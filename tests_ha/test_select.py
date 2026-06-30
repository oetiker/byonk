from tests_ha.conftest import make_device_entry, make_hub_entry

DEV = {"key": "AA:BB", "registered": True, "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [
        {"name": "transit", "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None},
        {"name": "weather", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson", "sierra"]}


async def test_screen_select_resets_params_to_defaults(hass, byonk):
    byonk.devices = [DEV]
    byonk.screens = SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    ent = next(s for s in hass.states.async_all("select") if "trmnl" in s.entity_id and "screen" in s.entity_id)
    await hass.services.async_call(
        "select", "select_option",
        {"entity_id": ent.entity_id, "option": "weather"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload["screen"] == "weather"
    assert payload["params"] == {}  # weather has no defaults
