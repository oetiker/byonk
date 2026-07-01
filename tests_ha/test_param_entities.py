from tests_ha.conftest import make_device_entry, make_hub_entry

SCREENS = {
    "screens": [
        {"name": "transit", "params": [{"name": "station", "type": "string"}], "schema_error": None},
        {"name": "floerli", "params": [{"name": "room", "type": "string"}], "schema_error": None},
        {"name": "calibrator", "params": [], "schema_error": None},
    ],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


async def _setup(hass, byonk, screen, params):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": screen, "params": params}]
    byonk.screens = SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_string_param_is_text_entity(hass, byonk):
    await _setup(hass, byonk, "transit", {"station": "Olten"})
    st = hass.states.get("text.trmnl_aa_bb_station")
    assert st is not None
    assert st.state == "Olten"


async def test_no_param_entities_for_paramless_screen(hass, byonk):
    await _setup(hass, byonk, "calibrator", {})
    assert hass.states.get("text.trmnl_aa_bb_station") is None


async def test_text_param_write_sends_full_params(hass, byonk):
    await _setup(hass, byonk, "transit", {"station": "Olten"})
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": "text.trmnl_aa_bb_station", "value": "Bern"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"params": {"station": "Bern"}}


async def test_param_entities_reconcile_on_screen_change(hass, byonk):
    hub = await _setup(hass, byonk, "transit", {"station": "Olten"})
    assert hass.states.get("text.trmnl_aa_bb_station") is not None
    # device switched to floerli
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": "floerli", "params": {"room": "Kitchen"}}]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert hass.states.get("text.trmnl_aa_bb_station") is None
    assert hass.states.get("text.trmnl_aa_bb_room") is not None
    assert hass.states.get("text.trmnl_aa_bb_room").state == "Kitchen"
