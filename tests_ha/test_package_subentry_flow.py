from tests_ha.conftest import make_hub_entry


async def _setup(hass, byonk):
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_add_package_posts_and_creates_subentry(hass, byonk):
    hub = await _setup(hass, byonk)
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "user"}
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"],
        {"handle": "weather", "repo": "github.com/acme/screens", "pin": "main", "token": "s3cr3t"},
    )
    assert result["type"] == "create_entry"
    assert byonk.add_package.await_args.args[0] == {
        "handle": "weather", "repo": "github.com/acme/screens", "pin": "main", "token": "s3cr3t",
    }
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    assert "token" not in sub.data  # token never persisted


async def test_add_package_surfaces_byonk_error(hass, byonk):
    from custom_components.byonk.api import ByonkValidationError
    byonk.add_package.side_effect = ByonkValidationError("package `weather` already exists")
    hub = await _setup(hass, byonk)
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "user"}
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": "r", "pin": "main"}
    )
    assert result["type"] == "form"
    assert result["errors"]["base"] == "add_failed"
    assert not any(s.unique_id == "weather" for s in hub.subentries.values())


PKG = {"handle": "weather", "builtin": False, "repo": "github.com/acme/screens",
       "pin": "main", "pin_kind": "branch", "resolved_sha": "abc",
       "status": "ready", "last_fetched": None, "error": None}


async def _hub_with_pkg(hass, byonk):
    from homeassistant.config_entries import ConfigSubentry, ConfigSubentryData
    byonk.packages = [PKG]
    hub = make_hub_entry(hass)
    hub.subentries = {}
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    # NOTE: Task 5 (coordinator reconcile) is not yet implemented, so the
    # "weather" subentry is not created automatically from byonk.packages.
    # Create it explicitly here; once Task 5 lands, the reconcile creates it
    # and this explicit creation can be removed.
    if not any(s.unique_id == "weather" for s in hub.subentries.values()):
        hass.config_entries.async_add_subentry(
            hub,
            ConfigSubentry(
                data=ConfigSubentryData(
                    {"handle": "weather", "repo": PKG["repo"], "pin": PKG["pin"]}
                ),
                subentry_type="package",
                title=f'weather — {PKG["repo"]}',
                unique_id="weather",
            ),
        )
    return hub


async def test_reconfigure_patches_pin(hass, byonk):
    hub = await _hub_with_pkg(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "reconfigure",
        "subentry_id": sub.subentry_id},
    )
    result = await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": PKG["repo"], "pin": "v2.0.0"}
    )
    assert result["type"] == "abort"
    assert byonk.update_package.await_args.args[0] == "weather"
    assert byonk.update_package.await_args.args[1]["pin"] == "v2.0.0"


async def test_reconfigure_blank_token_omits_token(hass, byonk):
    hub = await _hub_with_pkg(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    result = await hass.config_entries.subentries.async_init(
        (hub.entry_id, "package"), context={"source": "reconfigure",
        "subentry_id": sub.subentry_id},
    )
    await hass.config_entries.subentries.async_configure(
        result["flow_id"], {"handle": "weather", "repo": PKG["repo"], "pin": "main"}
    )
    assert "token" not in byonk.update_package.await_args.args[1]
