from tests_ha.conftest import make_hub_entry

REPOS = [
    {"handle": "byonk-builtin", "builtin": True, "status": "ready"},
    {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
     "pin_kind": "branch", "resolved_sha": "abc123", "status": "ready",
     "last_fetched": "2026-07-04T00:00:00+00:00", "error": None},
]


async def test_status_sensor_reflects_screen_repo(hass, byonk):
    byonk.screen_repos = REPOS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    state = hass.states.get("sensor.byonk_weather_status")
    assert state is not None
    assert state.state == "ready"
    assert state.attributes["resolved_sha"] == "abc123"
    # builtin has no status sensor
    assert hass.states.get("sensor.byonk_byonk_builtin_status") is None


async def test_status_sensor_added_and_removed(hass, byonk):
    byonk.screen_repos = [REPOS[0]]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get("sensor.byonk_weather_status") is None
    byonk.screen_repos = REPOS
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert hass.states.get("sensor.byonk_weather_status") is not None
