from custom_components.byonk.const import BUILTIN_SCREEN_LABEL
from tests_ha.conftest import make_hub_entry

TRANSIT_REF = "byonk-builtin/useful/swiss-departure-board"


async def test_options_flow_writes_settings(hass, byonk):
    byonk.config = {"registration": {"enabled": True, "screen": ""},
                    "auth_mode": "api_key", "package_refresh_interval": 3600}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    result = await hass.config_entries.options.async_init(hub.entry_id)
    assert result["type"] == "form"
    result = await hass.config_entries.options.async_configure(
        result["flow_id"],
        {"registration_screen": TRANSIT_REF, "auth_mode": "ed25519",
         "package_refresh_interval": 900},
    )
    assert result["type"] == "create_entry"
    sent = byonk.update_settings.await_args.args[0]
    assert sent["registration_screen"] == TRANSIT_REF
    assert sent["auth_mode"] == "ed25519"
    assert sent["package_refresh_interval"] == 900


async def test_options_flow_builtin_screen_maps_to_empty(hass, byonk):
    byonk.config = {"registration": {"enabled": True, "screen": ""},
                    "auth_mode": "api_key", "package_refresh_interval": 0}
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()

    result = await hass.config_entries.options.async_init(hub.entry_id)
    assert result["type"] == "form"
    result = await hass.config_entries.options.async_configure(
        result["flow_id"],
        {"registration_screen": BUILTIN_SCREEN_LABEL, "auth_mode": "api_key",
         "package_refresh_interval": 0},
    )
    assert result["type"] == "create_entry"
    assert byonk.update_settings.await_args.args[0]["registration_screen"] == ""
