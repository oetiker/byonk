from homeassistant.helpers import device_registry as dr

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import make_device_entry, make_hub_entry


async def _setup(hass, byonk):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": "byonk-builtin/useful/swiss-departure-board"}]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    return dev_entry


async def test_rename_syncs_to_byonk(hass, byonk):
    await _setup(hass, byonk)
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, "AA:BB")})
    assert device is not None

    byonk.update_device.reset_mock()
    registry.async_update_device(device.id, name_by_user="Kitchen")
    await hass.async_block_till_done()

    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"name": "Kitchen"}


async def test_clear_name_syncs_empty(hass, byonk):
    await _setup(hass, byonk)
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, "AA:BB")})
    registry.async_update_device(device.id, name_by_user="Kitchen")
    await hass.async_block_till_done()

    byonk.update_device.reset_mock()
    registry.async_update_device(device.id, name_by_user=None)
    await hass.async_block_till_done()

    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"name": ""}
