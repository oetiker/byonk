from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState, ConfigSubentry
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]
SCREENS = {"screens": [{"name": "transit",
            "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None}],
           "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"]}
SCREENS_NO_PARAMS = {"screens": [{"name": "transit", "params": [], "schema_error": None}],
                     "panels": [], "dither_algorithms": ["atkinson"]}

DEV = {"key": "AA:BB", "registered": True, "model": "og",
       "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
       "firmware_version": "1.7.1", "screen": "transit", "params": {},
       "dither": "atkinson", "panel": None}


async def _setup(hass, add_device, screens=None):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=screens or SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_add_device_posts_and_creates_subentry(hass):
    """FIX 2: device option value is MAC, not registration code."""
    add_device = AsyncMock(return_value={"key": "CC:DD", "screen": "transit"})
    entry = await _setup(hass, add_device)
    with patch("custom_components.byonk.coordinator.ByonkClient.async_add_device", new=add_device):
        result = await hass.config_entries.subentries.async_init(
            (entry.entry_id, "device"), context={"source": "user"}
        )
        # Submit the MAC (CC:DD) as the key — the option VALUE is now the MAC
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"key": "CC:DD", "screen": "transit"}
        )
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"limit": 5}
        )
    # Key submitted to add_device must be the MAC
    assert add_device.await_args.args[0]["key"] == "CC:DD"
    assert add_device.await_args.args[0]["params"] == {"limit": 5}
    assert any(s.unique_id == "CC:DD" for s in entry.subentries.values())


async def _setup_with_device(hass, screens=None):
    """Set up the entry with one registered device so reconcile creates a subentry."""
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=screens or SCREENS_NO_PARAMS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_reconfigure_no_params_skips_form(hass):
    """FIX 3: reconfigure with zero-params screen aborts without showing a form."""
    update_device = AsyncMock()
    entry = await _setup_with_device(hass, screens=SCREENS_NO_PARAMS)

    sub = next(s for s in entry.subentries.values() if s.subentry_type == "device")

    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS_NO_PARAMS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_device", new=update_device),
    ):
        result = await hass.config_entries.subentries.async_init(
            (entry.entry_id, "device"),
            context={"source": "reconfigure", "subentry_id": sub.subentry_id},
        )
        await hass.async_block_till_done()

    # No-params shortcut: should abort immediately without showing a form
    assert result["type"] == "abort"
    assert result["reason"] == "reconfigure_successful"
    # async_update_device must have been called (empty params)
    assert update_device.called
    key, payload = update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"screen": "transit", "params": {}}


async def test_reconfigure_with_params_shows_form_then_patches(hass):
    """FIX 3: reconfigure with a params screen shows form, then patches on submit."""
    update_device = AsyncMock()
    entry = await _setup_with_device(hass, screens=SCREENS)

    sub = next(s for s in entry.subentries.values() if s.subentry_type == "device")

    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_device", new=update_device),
    ):
        # First call: should show the reconfigure form (has params)
        result = await hass.config_entries.subentries.async_init(
            (entry.entry_id, "device"),
            context={"source": "reconfigure", "subentry_id": sub.subentry_id},
        )
        assert result["type"] == "form"
        assert result["step_id"] == "reconfigure"

        # Second call: submit params → should patch and abort
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"limit": 10}
        )
        await hass.async_block_till_done()

    assert result["type"] == "abort"
    assert result["reason"] == "reconfigure_successful"
    assert update_device.called
    key, payload = update_device.await_args.args
    assert key == "AA:BB"
    assert payload["params"] == {"limit": 10}
    assert payload["screen"] == "transit"
