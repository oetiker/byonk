from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {"key": "AA:BB", "registered": True, "screen": "byonk-builtin/useful/swiss-departure-board", "refresh": 600}


async def _setup(hass, byonk):
    byonk.devices = [DEV]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()


def _refresh(hass):
    return next(
        s for s in hass.states.async_all("text")
        if "trmnl" in s.entity_id and s.entity_id.endswith("_refresh")
    )


async def test_refresh_reflects_value(hass, byonk):
    await _setup(hass, byonk)
    assert _refresh(hass).state == "600"


async def test_refresh_sets_value(hass, byonk):
    await _setup(hass, byonk)
    ent = _refresh(hass)
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": ent.entity_id, "value": "300"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"refresh": 300}
