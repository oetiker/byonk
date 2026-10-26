from tests_ha.conftest import make_device_entry, make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"
WEATHER_REF = "byonk-builtin/useful/weather"
DEV = {"key": "AA:BB", "registered": True, "screen": TRANSIT_REF, "dither": "atkinson", "panel": None}
SCREENS = {
    "screen_repos": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [
                      {"ref": TRANSIT_REF, "title": "Swiss Departure Board", "description": "",
                       "params": [{"name": "limit", "type": "int", "default": 8}],
                       "byonk": "0.15", "compat_warning": None},
                      {"ref": WEATHER_REF, "title": "Weather", "description": "",
                       "params": [], "byonk": "0.15", "compat_warning": None},
                  ]}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson", "sierra"]}


async def test_panel_select_shows_value_not_in_options(hass, byonk):
    """A stored panel/dither byonk reports must show even if it is not a
    current option, instead of rendering as "unknown" (HA blanks a select
    whose current_option is absent from options)."""
    byonk.devices = [
        {
            "key": "AA:BB",
            "registered": True,
            "screen": TRANSIT_REF,
            "dither": "atkinson",
            "panel": "reterminal_e1002",  # not in SCREENS["panels"]
        }
    ]
    byonk.screens = SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()

    panel = next(
        s for s in hass.states.async_all("select")
        if "trmnl" in s.entity_id and "panel" in s.entity_id
    )
    assert panel.state == "reterminal_e1002"
    # The real option is still offered alongside the stored value.
    assert "trmnl_og" in panel.attributes["options"]
    assert "reterminal_e1002" in panel.attributes["options"]


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
        {"entity_id": ent.entity_id, "option": WEATHER_REF}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload["screen"] == WEATHER_REF
    assert payload["params"] == {}  # weather has no defaults


async def _setup_dither(hass, byonk, device):
    byonk.devices = [device]
    byonk.screens = SCREENS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    dev_entry = make_device_entry(hass, hub, "AA:BB")
    await hass.config_entries.async_setup(dev_entry.entry_id)
    await hass.async_block_till_done()
    return next(
        s for s in hass.states.async_all("select")
        if "trmnl" in s.entity_id and "dither" in s.entity_id
    )


async def test_dither_select_writes_the_final_rung(hass, byonk):
    """The Dither control is the owner's veto and must stay one: a bare
    `dither` sits below the screen, so the entity would show a value the
    device does not render."""
    ent = await _setup_dither(hass, byonk, DEV)
    await hass.services.async_call(
        "select", "select_option",
        {"entity_id": ent.entity_id, "option": "sierra"}, blocking=True,
    )
    key, payload = byonk.update_device.await_args.args
    assert key == "AA:BB"
    assert payload == {"final": {"dither": "sierra"}}


async def test_dither_select_shows_the_veto_not_the_base_default(hass, byonk):
    ent = await _setup_dither(
        hass, byonk,
        {**DEV, "dither": "sierra", "final": {"dither": "atkinson"}},
    )
    assert ent.state == "atkinson"


async def test_dither_select_ignores_a_bare_dither_without_a_veto(hass, byonk):
    """A base-rung default is not in effect against a screen, so the entity
    must not claim it."""
    ent = await _setup_dither(hass, byonk, {**DEV, "dither": "sierra"})
    assert ent.state == "unknown"


async def test_dither_select_tolerates_a_null_final_block(hass, byonk):
    """A hand-edited `final:` with nothing under it arrives as null."""
    ent = await _setup_dither(hass, byonk, {**DEV, "final": None})
    assert ent.state == "unknown"
