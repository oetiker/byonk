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
