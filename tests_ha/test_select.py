from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

DEV = {"key": "AA:BB", "registered": True, "screen": "transit", "dither": "atkinson", "panel": None}
SCREENS = {"screens": [
        {"name": "transit", "params": [{"name": "limit", "type": "int", "default": 8}], "schema_error": None},
        {"name": "weather", "params": [], "schema_error": None}],
    "panels": [{"name": "trmnl_og"}], "dither_algorithms": ["atkinson", "sierra"]}


async def _setup(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    return entry


async def test_screen_select_resets_params_to_defaults(hass):
    entry = await _setup(hass)
    update = AsyncMock(return_value={})
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[DEV])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_device", new=update),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
        ent = next(s for s in hass.states.async_all("select") if "trmnl" in s.entity_id and "screen" in s.entity_id)
        await hass.services.async_call(
            "select", "select_option",
            {"entity_id": ent.entity_id, "option": "weather"}, blocking=True,
        )
    key, payload = update.await_args.args
    assert key == "AA:BB"
    assert payload["screen"] == "weather"
    assert payload["params"] == {}  # weather has no defaults
