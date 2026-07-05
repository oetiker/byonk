"""Tests for the reserved DEFAULT device: auto-provision, orphan-prune exemption,
and the screen-only select surface."""
from unittest.mock import patch

from homeassistant.helpers import entity_registry as er

from custom_components.byonk.const import CONF_DEVICE_KEY, DEFAULT_DEVICE_KEY, DOMAIN
from custom_components.byonk.coordinator import ByonkData
from tests_ha.conftest import make_device_entry, make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"
DEFAULT_SCREEN = "byonk-builtin/default"
WEATHER_REF = "byonk-builtin/useful/weather"

DEFAULT_DEV = {
    "key": DEFAULT_DEVICE_KEY,
    "registered": True,
    "reserved": True,
    "screen": DEFAULT_SCREEN,
    "dither": None,
    "panel": None,
    "model": None,
    "firmware_version": None,
    "last_seen": None,
    "battery_voltage": None,
    "rssi": None,
    "params": {},
    "refresh": None,
    "name": None,
}

SCREENS = {
    "packages": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [
                      {"ref": DEFAULT_SCREEN, "title": "Default", "description": "",
                       "params": [], "byonk": "0.15", "compat_warning": None},
                      {"ref": TRANSIT_REF, "title": "Swiss Departure Board", "description": "",
                       "params": [], "byonk": "0.15", "compat_warning": None},
                      {"ref": WEATHER_REF, "title": "Weather", "description": "",
                       "params": [], "byonk": "0.15", "compat_warning": None},
                  ]}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson"],
}


async def _setup_hub(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_default_device_entry_is_provisioned(hass, byonk):
    byonk.devices = [DEFAULT_DEV]
    byonk.screens = SCREENS
    await _setup_hub(hass)
    # The auto-provision flow is scheduled via async_create_task(eager_start=False);
    # give the loop another tick to run it to completion.
    await hass.async_block_till_done()

    entries = [
        e for e in hass.config_entries.async_entries(DOMAIN)
        if e.unique_id == DEFAULT_DEVICE_KEY
    ]
    assert len(entries) == 1
    entry = entries[0]
    assert entry.data[CONF_DEVICE_KEY] == DEFAULT_DEVICE_KEY

    registry = er.async_get(hass)
    assert registry.async_get_entity_id("select", DOMAIN, f"{DEFAULT_DEVICE_KEY}_screen") is not None
    # dither/panel selects are NOT exposed for the reserved DEFAULT device.
    assert registry.async_get_entity_id("select", DOMAIN, f"{DEFAULT_DEVICE_KEY}_dither") is None
    assert registry.async_get_entity_id("select", DOMAIN, f"{DEFAULT_DEVICE_KEY}_panel") is None


async def test_default_device_not_reprovisioned_twice(hass, byonk):
    byonk.devices = [DEFAULT_DEV]
    byonk.screens = SCREENS
    hub = await _setup_hub(hass)
    await hass.async_block_till_done()

    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()

    entries = [
        e for e in hass.config_entries.async_entries(DOMAIN)
        if e.unique_id == DEFAULT_DEVICE_KEY
    ]
    assert len(entries) == 1


async def test_default_device_exempt_from_orphan_prune(hass, byonk):
    # byonk reports DEFAULT registered but (transiently) no HA entry exists yet ->
    # the coordinator must never attempt to orphan-prune the reserved device.
    hub = await _setup_hub(hass)
    coordinator = hub.runtime_data
    data = ByonkData(
        devices=[DEFAULT_DEV],
        pending=[],
        screens=[],
        panels=[],
        dither=[],
        config={},
        packages=[],
    )
    # REMOVE_STRIKES is 2; call twice to make sure a real orphan would have been
    # pruned by now, but DEFAULT never is.
    await coordinator._async_reconcile(data)
    await coordinator._async_reconcile(data)
    assert not byonk.delete_device.called


async def test_removing_default_entry_does_not_delete_byonk_mapping(hass, byonk):
    # A manual "Delete device" from Settings -> Devices & Services on the
    # reserved DEFAULT entry must be harmless: byonk never loses the DEFAULT
    # mapping, so the next coordinator refresh can re-provision the entry.
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, DEFAULT_DEVICE_KEY)
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    with patch(
        "custom_components.byonk.ByonkClient.async_delete_device", new=byonk.delete_device
    ):
        await hass.config_entries.async_remove(dev_entry.entry_id)
        await hass.async_block_till_done()
    assert not byonk.delete_device.called


async def test_default_device_screen_select_writes_patch(hass, byonk):
    byonk.devices = [DEFAULT_DEV]
    byonk.screens = SCREENS
    await _setup_hub(hass)
    await hass.async_block_till_done()

    registry = er.async_get(hass)
    entity_id = registry.async_get_entity_id("select", DOMAIN, f"{DEFAULT_DEVICE_KEY}_screen")
    assert entity_id is not None

    await hass.services.async_call(
        "select", "select_option",
        {"entity_id": entity_id, "option": WEATHER_REF}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == DEFAULT_DEVICE_KEY
    assert payload["screen"] == WEATHER_REF
