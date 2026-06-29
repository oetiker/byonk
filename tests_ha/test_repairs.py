from unittest.mock import AsyncMock, patch

from homeassistant.helpers import issue_registry as ir
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN
from custom_components.byonk.repairs import async_sync_pending_issues

PENDING = [{"mac": "CC:DD", "registration_code": "ABCD-1234", "model": "og", "last_seen": None}]
SCREENS = {"screens": [], "panels": [], "dither_algorithms": []}


async def test_pending_creates_repair_issue(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=PENDING)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    reg = ir.async_get(hass)
    assert reg.async_get_issue(DOMAIN, "device_pending_ABCD-1234") is not None


async def test_resolve_pending_deletes_issue(hass):
    """After an issue is created, passing an empty pending list must delete it."""
    # Create the issue first
    async_sync_pending_issues(hass, PENDING)
    reg = ir.async_get(hass)
    assert reg.async_get_issue(DOMAIN, "device_pending_ABCD-1234") is not None

    # Now resolve — empty pending list must delete the issue
    async_sync_pending_issues(hass, [])
    assert reg.async_get_issue(DOMAIN, "device_pending_ABCD-1234") is None


async def test_mac_fallback_creates_issue_with_mac_id(hass):
    """A pending entry with no registration_code but a mac uses the mac as issue id."""
    pending_mac_only = [{"mac": "EE:FF:00:11", "model": "og", "last_seen": None}]
    async_sync_pending_issues(hass, pending_mac_only)
    reg = ir.async_get(hass)
    assert reg.async_get_issue(DOMAIN, "device_pending_EE:FF:00:11") is not None
