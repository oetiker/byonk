from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {"key": "AA:BB", "registered": True, "screen": "transit", "refresh": 600}


async def _setup(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()


async def test_refresh_number_reflects_value(hass, byonk):
    await _setup(hass, byonk)
    ent = next(
        s for s in hass.states.async_all("number")
        if "trmnl" in s.entity_id and "refresh" in s.entity_id
    )
    assert int(float(ent.state)) == 600


async def test_refresh_number_sets_value(hass, byonk):
    await _setup(hass, byonk)
    ent = next(
        s for s in hass.states.async_all("number")
        if "trmnl" in s.entity_id and "refresh" in s.entity_id
    )
    await hass.services.async_call(
        "number", "set_value",
        {"entity_id": ent.entity_id, "value": 300}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"refresh": 300}
