from homeassistant.config_entries import SOURCE_INTEGRATION_DISCOVERY

from custom_components.byonk.const import CONF_DEVICE_KEY, DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {
    "key": "AA:BB", "registered": True, "model": "og",
    "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
    "firmware_version": "1.7.1", "screen": "transit", "dither": "atkinson", "panel": None,
}
PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og",
            "last_seen": "2026-06-29T09:00:00+00:00"}]


async def _setup_hub(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_pending_injects_discovery_flow(hass, byonk):
    byonk.pending = PENDING
    await _setup_hub(hass)
    await hass.async_block_till_done()
    flows = hass.config_entries.flow.async_progress_by_handler(DOMAIN)
    disc = [f for f in flows if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert len(disc) == 1


async def test_no_duplicate_discovery_flow(hass, byonk):
    byonk.pending = PENDING
    hub = await _setup_hub(hass)
    coordinator = hub.runtime_data
    await coordinator.async_refresh()
    await hass.async_block_till_done()
    disc = [f for f in hass.config_entries.flow.async_progress_by_handler(DOMAIN)
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert len(disc) == 1


async def test_discovery_flow_torn_down_when_no_longer_pending(hass, byonk):
    byonk.pending = PENDING
    hub = await _setup_hub(hass)
    assert hass.config_entries.flow.async_progress_by_handler(DOMAIN)
    byonk.pending = []
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    disc = [f for f in hass.config_entries.flow.async_progress_by_handler(DOMAIN)
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY]
    assert not disc


async def test_device_removed_after_two_strikes(hass, byonk):
    byonk.devices = [DEV]
    hub = await _setup_hub(hass)
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    coordinator = hub.runtime_data
    byonk.devices = []  # device deregistered in byonk

    await coordinator.async_refresh()  # strike 1
    await hass.async_block_till_done()
    assert hass.config_entries.async_get_entry(dev_entry.entry_id) is not None

    await coordinator.async_refresh()  # strike 2
    await hass.async_block_till_done()
    assert hass.config_entries.async_get_entry(dev_entry.entry_id) is None


async def test_orphan_byonk_mapping_pruned_after_two_strikes(hass, byonk):
    # byonk reports a registered device that HA has no entry for
    byonk.devices = [DEV]
    hub = await _setup_hub(hass)
    coordinator = hub.runtime_data

    await coordinator.async_refresh()  # strike 1 — not deleted yet
    await hass.async_block_till_done()
    assert not byonk.delete_device.called

    await coordinator.async_refresh()  # strike 2 — pruned
    await hass.async_block_till_done()
    assert byonk.delete_device.await_args.args[0] == "AA:BB"
