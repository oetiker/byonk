from unittest.mock import patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {
    "key": "AA:BB", "registered": True, "model": "og",
    "battery_voltage": 4.1, "rssi": -58, "last_seen": "2026-06-29T10:00:00+00:00",
    "firmware_version": "1.7.1", "screen": "byonk-builtin/useful/swiss-departure-board", "params": {},
    "dither": "atkinson", "panel": None,
}


async def test_device_entry_creates_device_and_entities(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    assert dev_entry.state is ConfigEntryState.LOADED
    # the device's diagnostic sensors exist
    assert hass.states.get("sensor.trmnl_aa_bb_battery_voltage") is not None


async def test_device_entry_not_ready_without_hub(hass, byonk):
    # Do NOT add hub to hass: HA auto-sets up ALL domain entries when the domain is
    # first loaded (async_setup_component), so a registered hub would be set up
    # alongside the device entry, defeating the "hub not ready" scenario.
    hub = MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        title="Byonk",
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    # hub coordinator absent from hass.data -> device setup retries
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    assert dev_entry.state is ConfigEntryState.SETUP_RETRY


async def test_device_entry_rebinds_after_hub_reload(hass, byonk):
    """Regression: a hub reload (e.g. after a re-auth token re-provision) builds a
    brand-new coordinator. A device entry that stayed LOADED must be reloaded so it
    rebinds to the live coordinator; otherwise its entities are stuck unavailable."""
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    assert dev_entry.state is ConfigEntryState.LOADED
    old_coord = dev_entry.runtime_data

    # Reload the hub -> a new coordinator object replaces the old one.
    await hass.config_entries.async_reload(hub.entry_id)
    await hass.async_block_till_done()

    new_coord = hass.data[DOMAIN][hub.entry_id]
    assert new_coord is not old_coord, "hub reload should build a fresh coordinator"
    assert dev_entry.state is ConfigEntryState.LOADED
    assert dev_entry.runtime_data is new_coord, "device entry must rebind to the live coordinator"
    state = hass.states.get("sensor.trmnl_aa_bb_battery_voltage")
    assert state is not None and state.state != "unavailable"


async def test_remove_device_entry_deletes_byonk_mapping(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    with patch(
        "custom_components.byonk.ByonkClient.async_delete_device", new=byonk.delete_device
    ):
        await hass.config_entries.async_remove(dev_entry.entry_id)
        await hass.async_block_till_done()
    assert byonk.delete_device.await_args.args[0] == "AA:BB"
