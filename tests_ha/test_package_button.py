from tests_ha.conftest import make_hub_entry


async def test_update_packages_button(hass, byonk):
    byonk.packages = [{"handle": "weather", "builtin": False, "status": "ready"}]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    ent = "button.byonk_update_packages"
    assert hass.states.get(ent) is not None
    await hass.services.async_call(
        "button", "press", {"entity_id": ent}, blocking=True
    )
    assert byonk.update_packages.await_count == 1
