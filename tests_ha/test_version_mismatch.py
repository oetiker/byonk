"""The app rewrites custom_components/byonk whenever it starts, but Home
Assistant reads that directory only at boot. After an app update the running
integration is a version behind until the user restarts, so say so."""
from homeassistant.components.hassio import AddonError
from homeassistant.helpers import issue_registry as ir

from custom_components.byonk.const import DOMAIN
from tests_ha.conftest import INTEGRATION_VERSION, make_hub_entry

ISSUE_ID = "version_mismatch"


async def _setup(hass):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_no_issue_when_versions_agree(hass, byonk):
    await _setup(hass)
    assert ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID) is None


async def test_issue_raised_and_cleared(hass, byonk):
    byonk.addon_version = "999.0.0"
    hub = await _setup(hass)

    reg = ir.async_get(hass)
    issue = reg.async_get_issue(DOMAIN, ISSUE_ID)
    assert issue is not None
    assert issue.severity == ir.IssueSeverity.WARNING
    assert issue.translation_placeholders == {
        "addon": "999.0.0",
        "integration": INTEGRATION_VERSION,
    }

    # The user restarts Home Assistant; the app and the integration now agree.
    byonk.addon_version = INTEGRATION_VERSION
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert reg.async_get_issue(DOMAIN, ISSUE_ID) is None


async def test_supervisor_failure_is_silent(hass, byonk):
    """A Supervisor hiccup must not invent a version mismatch."""
    from unittest.mock import AsyncMock, patch

    with patch(
        "custom_components.byonk.coordinator.async_get_addon_version",
        new=AsyncMock(side_effect=AddonError("supervisor unavailable")),
    ):
        await _setup(hass)
    assert ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID) is None
