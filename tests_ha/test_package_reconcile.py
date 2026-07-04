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
