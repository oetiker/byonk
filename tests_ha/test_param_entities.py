from homeassistant.helpers import entity_registry as er

from tests_ha.conftest import make_device_entry, make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"
FLOERLI_REF = "byonk-builtin/demos/floerli"
CALIBRATOR_REF = "byonk-builtin/utils/calibrator"
GPHOTO_REF = "byonk-builtin/demos/gphoto"

SCREENS = {
    "packages": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [
                      {"ref": TRANSIT_REF, "title": "Swiss Departure Board", "description": "",
                       "params": [{"name": "station", "type": "string"}],
                       "byonk": "0.15", "compat_warning": None},
                      {"ref": FLOERLI_REF, "title": "Floerli", "description": "",
                       "params": [{"name": "room", "type": "string"}],
                       "byonk": "0.15", "compat_warning": None},
                      {"ref": CALIBRATOR_REF, "title": "Calibrator", "description": "",
                       "params": [], "byonk": "0.15", "compat_warning": None},
                  ]}],
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
    await _setup(hass, byonk, TRANSIT_REF, {"station": "Olten"})
    st = hass.states.get("text.trmnl_aa_bb_station")
    assert st is not None
    assert st.state == "Olten"


async def test_no_param_entities_for_paramless_screen(hass, byonk):
    await _setup(hass, byonk, CALIBRATOR_REF, {})
    assert hass.states.get("text.trmnl_aa_bb_station") is None


async def test_text_param_write_sends_single_key_and_shows_immediately(hass, byonk):
    await _setup(hass, byonk, TRANSIT_REF, {"station": "Olten"})
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": "text.trmnl_aa_bb_station", "value": "Bern"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    # only this field is sent; byonk merges it (other params preserved server-side)
    assert payload == {"params": {"station": "Bern"}}
    # optimistic: the new value shows immediately, before any coordinator refresh
    assert hass.states.get("text.trmnl_aa_bb_station").state == "Bern"


async def test_param_entities_reconcile_on_screen_change(hass, byonk):
    hub = await _setup(hass, byonk, TRANSIT_REF, {"station": "Olten"})
    assert hass.states.get("text.trmnl_aa_bb_station") is not None
    # device switched to floerli
    byonk.devices = [{"key": "AA:BB", "registered": True, "screen": FLOERLI_REF, "params": {"room": "Kitchen"}}]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert hass.states.get("text.trmnl_aa_bb_station") is None
    assert hass.states.get("text.trmnl_aa_bb_room") is not None
    assert hass.states.get("text.trmnl_aa_bb_room").state == "Kitchen"
    # The removed entity must be gone from the registry too (not just the state),
    # otherwise it lingers on the device page as an "unavailable" entity.
    registry = er.async_get(hass)
    assert registry.async_get("text.trmnl_aa_bb_station") is None


NUM_SCREENS = {
    "packages": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [
                      {"ref": TRANSIT_REF, "title": "Swiss Departure Board", "description": "",
                       "params": [
                           {"name": "station", "type": "string"},
                           {"name": "limit", "type": "int", "min": 1, "max": 30},
                       ], "byonk": "0.15", "compat_warning": None},
                      {"ref": GPHOTO_REF, "title": "GPhoto", "description": "",
                       "params": [
                           {"name": "show_status", "type": "bool"},
                           {"name": "theme", "type": "enum", "options": [
                               {"value": "light", "label": "Light"}, {"value": "dark", "label": "Dark"}]},
                       ], "byonk": "0.15", "compat_warning": None},
                  ]}],
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


async def test_int_param_is_text_and_coerces_to_int(hass, byonk):
    # int/float params render as Text (label-above), but the value is coerced
    # back to a real int before it reaches byonk.
    await _setup_num(hass, byonk, TRANSIT_REF, {"station": "Olten", "limit": 8})
    st = hass.states.get("text.trmnl_aa_bb_limit")
    assert st is not None
    assert st.state == "8"
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": "text.trmnl_aa_bb_limit", "value": "12"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    # only the edited field is sent; byonk merges (station preserved server-side)
    assert payload == {"params": {"limit": 12}}
    assert isinstance(payload["params"]["limit"], int)
    assert hass.states.get("text.trmnl_aa_bb_limit").state == "12"


async def test_int_param_rejects_non_whole_number(hass, byonk):
    await _setup_num(hass, byonk, TRANSIT_REF, {"station": "Olten", "limit": 8})
    byonk.update_device.reset_mock()
    await hass.services.async_call(
        "text", "set_value",
        {"entity_id": "text.trmnl_aa_bb_limit", "value": "8.5"}, blocking=True,
    )
    # non-whole value for an int param is rejected locally; no write to byonk
    assert not byonk.update_device.await_count


async def test_bool_param_is_switch(hass, byonk):
    await _setup_num(hass, byonk, GPHOTO_REF, {"show_status": False, "theme": "light"})
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
    await _setup_num(hass, byonk, GPHOTO_REF, {"show_status": True, "theme": "sepia"})
    st = hass.states.get("select.trmnl_aa_bb_theme")
    assert st is not None
    assert st.state == "sepia"
    assert "light" in st.attributes["options"]
    assert "sepia" in st.attributes["options"]
