from unittest.mock import AsyncMock, patch

from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN
from tests_ha.conftest import make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"
SCREENS = {
    "screen_repos": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [{"ref": TRANSIT_REF, "title": "Swiss Departure Board", "description": "",
                                "params": [], "byonk": "0.15", "compat_warning": None}]}],
    "panels": [], "dither_algorithms": []}
CONFIG = {"registration": {"enabled": False}, "auth_mode": "api_key"}


async def test_registration_switch_turns_on(hass):
    entry = MockConfigEntry(domain=DOMAIN, unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"})
    entry.add_to_hass(hass)
    settings = AsyncMock(return_value={})
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value=CONFIG)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screen_repos", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_update_settings", new=settings),
    ):
        await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()
        ent = next(s for s in hass.states.async_all("switch") if "registration" in s.entity_id)
        assert ent.state == "off"
        await hass.services.async_call(
            "switch", "turn_on", {"entity_id": ent.entity_id}, blocking=True
        )
    assert settings.await_args.args[0] == {"registration_enabled": True}


async def test_hub_has_no_settings_selects(hass, byonk):
    byonk.config = {"registration": {"enabled": True}, "auth_mode": "api_key"}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    assert hass.states.get("select.byonk_new_device_screen") is None
    assert hass.states.get("select.byonk_auth_mode") is None
