from unittest.mock import AsyncMock, patch

from homeassistant import config_entries
from homeassistant.data_entry_flow import FlowResultType

from custom_components.byonk.const import CONF_BASE_URL, DOMAIN


async def _start(hass):
    return await hass.config_entries.flow.async_init(
        DOMAIN, context={"source": config_entries.SOURCE_USER}
    )


async def test_aborts_without_supervisor(hass):
    with patch("custom_components.byonk.config_flow.is_hassio", return_value=False):
        result = await _start(hass)
    assert result["type"] == FlowResultType.ABORT
    assert result["reason"] == "not_hassio"


async def test_happy_path_creates_entry_without_token(hass):
    with (
        patch("custom_components.byonk.config_flow.is_hassio", return_value=True),
        patch(
            "custom_components.byonk.config_flow.async_ensure_addon_installed",
            new=AsyncMock(return_value="abcd1234_byonk"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_read_token",
            new=AsyncMock(return_value=None),
        ),
        patch(
            "custom_components.byonk.config_flow.async_provision_token",
            new=AsyncMock(return_value="tok"),
        ),
        patch(
            "custom_components.byonk.config_flow.async_get_base_url",
            new=AsyncMock(return_value="http://addon:3000"),
        ),
        patch(
            "custom_components.byonk.config_flow.ByonkClient.async_get_config",
            new=AsyncMock(return_value={}),
        ),
    ):
        result = await _start(hass)
    assert result["type"] == FlowResultType.CREATE_ENTRY
    assert result["data"] == {"addon_slug": "abcd1234_byonk", CONF_BASE_URL: "http://addon:3000"}
    assert "admin_token" not in result["data"]
    assert "tok" not in str(result["data"])


async def test_addon_failure_aborts_gracefully(hass):
    from homeassistant.components.hassio import AddonError

    with (
        patch("custom_components.byonk.config_flow.is_hassio", return_value=True),
        patch(
            "custom_components.byonk.config_flow.async_ensure_addon_installed",
            new=AsyncMock(side_effect=AddonError("clone failed")),
        ),
    ):
        result = await _start(hass)
    assert result["type"] == FlowResultType.ABORT
    assert result["reason"] == "addon_error"
