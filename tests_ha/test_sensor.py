from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN
from homeassistant.helpers import entity_registry as er

DEV = {"key": "AA:BB", "registered": True, "model": "og", "battery_voltage": 4.1,
       "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00", "firmware_version": "1.7.1",
       "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [{"name": "transit", "params": [], "schema_error": None}],
           "panels": [], "dither_algorithms": ["atkinson"]}


async def test_battery_sensor_state(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    state = hass.states.get("sensor.aa_bb_battery_voltage") or _find(hass, "battery")
    assert state is not None
    assert float(state.state) == 4.1


def _find(hass, needle):
    for s in hass.states.async_all("sensor"):
        if needle in s.entity_id:
            return s
    return None


async def _setup_entry(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    return entry


async def test_last_seen_sensor_state(hass):
    """last_seen sensor must expose an ISO datetime state (TIMESTAMP device class)."""
    await _setup_entry(hass)
    state = _find(hass, "last_seen")
    assert state is not None
    # TIMESTAMP sensors expose their state as an ISO datetime string
    assert "2026-06-29" in state.state


async def test_firmware_version_sensor_state(hass):
    """firmware_version sensor must expose the firmware string."""
    await _setup_entry(hass)
    state = _find(hass, "firmware_version")
    assert state is not None
    assert state.state == "1.7.1"


async def test_model_sensor_state(hass):
    """model sensor must expose the model string."""
    await _setup_entry(hass)
    state = _find(hass, "model")
    assert state is not None
    assert state.state == "og"


async def test_rssi_sensor_disabled_by_default(hass):
    """rssi sensor must exist in entity registry but be disabled by default."""
    await _setup_entry(hass)
    registry = er.async_get(hass)
    key = DEV["key"]
    # Look up entity_id from the registry by unique_id
    entity_id = registry.async_get_entity_id("sensor", DOMAIN, f"{key}_rssi")
    assert entity_id is not None, "rssi entity must exist in entity registry"
    entry = registry.async_get(entity_id)
    assert entry is not None
    assert entry.disabled_by is not None, "rssi sensor must be disabled by default"
