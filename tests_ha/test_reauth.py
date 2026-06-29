from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN


async def test_reauth_reprovisions_when_blank(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    provision = AsyncMock(return_value="newtok")
    with (
        patch("custom_components.byonk.config_flow.async_read_token", new=AsyncMock(return_value=None)),
        patch("custom_components.byonk.config_flow.async_provision_token", new=provision),
    ):
        result = await entry.start_reauth_flow(hass)
        if result.get("type") == "form":
            result = await hass.config_entries.flow.async_configure(result["flow_id"], {})
    provision.assert_awaited_once()


async def test_reauth_no_provision_when_token_valid(hass):
    """When token exists and authenticates, provision must NOT be called."""
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    provision = AsyncMock(return_value="newtok")
    with (
        patch("custom_components.byonk.config_flow.async_read_token", new=AsyncMock(return_value="validtoken")),
        patch("custom_components.byonk.config_flow._token_authenticates", new=AsyncMock(return_value=True)),
        patch("custom_components.byonk.config_flow.async_provision_token", new=provision),
    ):
        result = await entry.start_reauth_flow(hass)
        if result.get("type") == "form":
            result = await hass.config_entries.flow.async_configure(result["flow_id"], {})
    provision.assert_not_called()


async def test_reauth_reprovisions_when_token_auth_fails(hass):
    """When token exists but auth probe fails, provision IS called."""
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    provision = AsyncMock(return_value="newtok")
    with (
        patch("custom_components.byonk.config_flow.async_read_token", new=AsyncMock(return_value="staletoken")),
        patch("custom_components.byonk.config_flow._token_authenticates", new=AsyncMock(return_value=False)),
        patch("custom_components.byonk.config_flow.async_provision_token", new=provision),
    ):
        result = await entry.start_reauth_flow(hass)
        if result.get("type") == "form":
            result = await hass.config_entries.flow.async_configure(result["flow_id"], {})
    provision.assert_awaited_once()
