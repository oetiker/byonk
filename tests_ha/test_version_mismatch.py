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


async def test_compares_against_the_loaded_integration_not_the_manifest_file(hass, byonk):
    """The comparison must go through the loader, never read manifest.json off
    disk. The app rewrites custom_components/byonk whenever it starts, so the
    file on disk is not the code Home Assistant is actually running; only the
    loader's cached, boot-time manifest is. If a future change swapped the
    loader call for a direct file read, this test would see the on-disk
    version (INTEGRATION_VERSION) instead of the patched one below, and the
    placeholder assertion would fail.
    """
    from types import SimpleNamespace
    from unittest.mock import AsyncMock, patch

    byonk.addon_version = "1.2.3"
    with patch(
        "custom_components.byonk.coordinator.async_get_integration",
        new=AsyncMock(return_value=SimpleNamespace(version="0.0.1")),
    ):
        await _setup(hass)

    issue = ir.async_get(hass).async_get_issue(DOMAIN, ISSUE_ID)
    assert issue is not None
    assert issue.translation_placeholders["integration"] == "0.0.1"
