from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

SCREENS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og", "width": 800, "height": 480, "colors": "bw"}],
    "dither_algorithms": ["atkinson"],
}
CONFIG = {"registration": {"enabled": True}, "default_screen": "transit", "auth_mode": "api_key"}


def _entry():
    return MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )


async def test_setup_entry_creates_hub_and_loads(hass):
    entry = _entry()
    entry.add_to_hass(hass)
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value=CONFIG)),
    ):
        assert await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
    assert entry.state is ConfigEntryState.LOADED
    assert entry.runtime_data.data.auth_mode() == "api_key"
