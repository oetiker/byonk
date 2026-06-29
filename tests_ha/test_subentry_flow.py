from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]
SCREENS = {"screens": [{"name": "transit",
            "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None}],
           "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"]}


async def _setup(hass, add_device):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_add_device_posts_and_creates_subentry(hass):
    add_device = AsyncMock(return_value={"key": "ABCD-1234", "screen": "transit"})
    entry = await _setup(hass, add_device)
    with patch("custom_components.byonk.coordinator.ByonkClient.async_add_device", new=add_device):
        result = await hass.config_entries.subentries.async_init(
            (entry.entry_id, "device"), context={"source": "user"}
        )
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"key": "ABCD-1234", "screen": "transit"}
        )
        result = await hass.config_entries.subentries.async_configure(
            result["flow_id"], {"limit": 5}
        )
    assert add_device.await_args.args[0]["key"] == "ABCD-1234"
    assert add_device.await_args.args[0]["params"] == {"limit": 5}
    assert any(s.unique_id == "ABCD-1234" for s in entry.subentries.values())
