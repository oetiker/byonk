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


NUM_SCREENS = {
    "screens": [
        {"name": "transit", "params": [
            {"name": "station", "type": "string"},
            {"name": "limit", "type": "int", "min": 1, "max": 30},
        ], "schema_error": None},
        {"name": "gphoto", "params": [
            {"name": "show_status", "type": "bool"},
            {"name": "theme", "type": "enum", "options": [
                {"value": "light", "label": "Light"}, {"value": "dark", "label": "Dark"}]},
        ], "schema_error": None},
    ],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


async def _setup_num(hass, byonk, screen, params):
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": screen, "params": params}]
    byonk.screens = NUM_SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_int_param_number_coerces_to_int(hass, byonk):
    await _setup_num(hass, byonk, "transit", {"station": "Olten", "limit": 8})
    st = hass.states.get("number.trmnl_aa_bb_limit")
    assert st is not None
    assert float(st.state) == 8.0
    await hass.services.async_call(
        "number", "set_value",
        {"entity_id": "number.trmnl_aa_bb_limit", "value": 12}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert payload["params"]["limit"] == 12
    assert isinstance(payload["params"]["limit"], int)
    # other params preserved
    assert payload["params"]["station"] == "Olten"


async def test_bool_param_is_switch(hass, byonk):
    await _setup_num(hass, byonk, "gphoto", {"show_status": False, "theme": "light"})
    st = hass.states.get("switch.trmnl_aa_bb_show_status")
    assert st is not None
    assert st.state == "off"
    await hass.services.async_call(
        "switch", "turn_on",
        {"entity_id": "switch.trmnl_aa_bb_show_status"}, blocking=True,
    )
    _key, payload = byonk.update_device.await_args.args
    assert payload["params"]["show_status"] is True


async def test_enum_param_select_includes_current_value(hass, byonk):
    # stored value not among declared options -> still shown, not "unknown"
    await _setup_num(hass, byonk, "gphoto", {"show_status": True, "theme": "sepia"})
    st = hass.states.get("select.trmnl_aa_bb_theme")
    assert st is not None
    assert st.state == "sepia"
    assert "light" in st.attributes["options"]
    assert "sepia" in st.attributes["options"]
