from tests_ha.conftest import make_hub_entry

PKGS = [
    {"handle": "byonk-builtin", "builtin": True, "status": "ready"},
    {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
     "pin_kind": "branch", "resolved_sha": "abc", "status": "ready",
     "last_fetched": "2026-07-04T00:00:00+00:00", "error": None},
]


async def test_coordinator_exposes_packages(hass, byonk):
    byonk.packages = PKGS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    data = hub.runtime_data.data
    assert [p["handle"] for p in data.non_builtin_packages()] == ["weather"]
    assert data.package("weather")["status"] == "ready"
    assert data.package("missing") is None


async def test_reconcile_creates_and_removes_subentries(hass, byonk):
    byonk.packages = PKGS  # builtin + weather
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    # weather present, builtin excluded
    handles = {s.unique_id for s in hub.subentries.values() if s.subentry_type == "package"}
    assert handles == {"weather"}

    # byonk drops weather -> reconcile removes the subentry on next refresh
    byonk.packages = [PKGS[0]]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    handles = {s.unique_id for s in hub.subentries.values() if s.subentry_type == "package"}
    assert handles == set()


async def test_reconcile_updates_title_on_pin_change(hass, byonk):
    byonk.packages = PKGS
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    byonk.packages = [PKGS[0], {**PKGS[1], "pin": "v9"}]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    assert sub.data["pin"] == "v9"
