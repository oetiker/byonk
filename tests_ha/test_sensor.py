from homeassistant.helpers import entity_registry as er

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {
    "key": "AA:BB", "registered": True, "model": "og", "battery_voltage": 4.1,
    "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00", "firmware_version": "1.7.1",
    "screen": "transit", "dither": "atkinson", "panel": None,
}


def _find(hass, needle):
    for s in hass.states.async_all("sensor"):
        if needle in s.entity_id:
            return s
    return None


async def _setup_entry(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    return hub, dev_entry


async def test_battery_sensor_state(hass, byonk):
    await _setup_entry(hass, byonk)
    state = hass.states.get("sensor.trmnl_aa_bb_battery_voltage")
    assert state is not None
    assert float(state.state) == 4.1


async def test_last_seen_sensor_state(hass, byonk):
    """last_seen sensor must expose an ISO datetime state (TIMESTAMP device class)."""
    await _setup_entry(hass, byonk)
    state = _find(hass, "last_seen")
    assert state is not None
    # TIMESTAMP sensors expose their state as an ISO datetime string
    assert "2026-06-29" in state.state


async def test_firmware_version_sensor_state(hass, byonk):
    """firmware_version sensor must expose the firmware string."""
    await _setup_entry(hass, byonk)
    state = _find(hass, "firmware_version")
    assert state is not None
    assert state.state == "1.7.1"


async def test_model_sensor_state(hass, byonk):
    """model sensor must expose the model string."""
    await _setup_entry(hass, byonk)
    state = _find(hass, "model")
    assert state is not None
    assert state.state == "og"


async def test_rssi_sensor_disabled_by_default(hass, byonk):
    """rssi sensor must exist in entity registry but be disabled by default."""
    await _setup_entry(hass, byonk)
    registry = er.async_get(hass)
    key = DEV["key"]
    # Look up entity_id from the registry by unique_id
    entity_id = registry.async_get_entity_id("sensor", DOMAIN, f"{key}_rssi")
    assert entity_id is not None, "rssi entity must exist in entity registry"
    entry = registry.async_get(entity_id)
    assert entry is not None
    assert entry.disabled_by is not None, "rssi sensor must be disabled by default"
