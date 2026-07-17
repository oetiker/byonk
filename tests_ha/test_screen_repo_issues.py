"""A failing screen-repo fetch surfaces as a repair issue, and clears on recovery."""
from homeassistant.helpers import issue_registry as ir

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import make_hub_entry

ISSUE_ID = "screen_repo_error_weather"

ERR_REPO = {
    "handle": "weather", "builtin": False, "repo": "https://example.com/w.git",
    "pin": "main", "status": "error", "resolved_sha": None,
    "error": "git error: could not resolve host",
}
OK_REPO = {**ERR_REPO, "status": "ready", "resolved_sha": "abc123", "error": None}


async def test_error_repo_raises_and_clears_issue(hass, byonk):
    byonk.screen_repos = [ERR_REPO]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    reg = ir.async_get(hass)
    issue = reg.async_get_issue(DOMAIN, ISSUE_ID)
    assert issue is not None
    assert issue.severity == ir.IssueSeverity.WARNING
    assert issue.translation_placeholders["error"] == ERR_REPO["error"]
    assert issue.translation_placeholders["handle"] == "weather"

    # Repo recovers -> issue is cleared.
    byonk.screen_repos = [OK_REPO]
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert reg.async_get_issue(DOMAIN, ISSUE_ID) is None


async def test_ready_repo_has_no_issue(hass, byonk):
    byonk.screen_repos = [OK_REPO]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID) is None
