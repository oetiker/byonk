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
