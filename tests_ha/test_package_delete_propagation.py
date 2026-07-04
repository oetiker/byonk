import logging
from tests_ha.conftest import make_hub_entry

PKG = {"handle": "weather", "builtin": False, "repo": "r", "pin": "main",
       "pin_kind": "branch", "resolved_sha": "abc", "status": "ready",
       "last_fetched": None, "error": None}


async def _hub(hass, byonk):
    byonk.packages = [PKG]
    hub = make_hub_entry(hass)
    await hass.config_entries.async_setup(hub.entry_id)
    await hass.async_block_till_done()
    return hub


async def test_removing_subentry_deletes_from_byonk(hass, byonk):
    hub = await _hub(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    byonk.packages = []  # byonk will report it gone after our delete
    hass.config_entries.async_remove_subentry(hub, sub.subentry_id)
    await hass.async_block_till_done()
    assert byonk.delete_package.await_args.args[0] == "weather"


async def test_delete_409_logs_and_self_heals(hass, byonk, caplog):
    from custom_components.byonk.api import ByonkReadOnlyError
    hub = await _hub(hass, byonk)
    sub = next(s for s in hub.subentries.values() if s.unique_id == "weather")
    byonk.delete_package.side_effect = ByonkReadOnlyError(
        "package `weather` is referenced by device `AA:BB`"
    )
    with caplog.at_level(logging.WARNING):
        hass.config_entries.async_remove_subentry(hub, sub.subentry_id)
        await hass.async_block_till_done()
    assert "referenced by device" in caplog.text
    # byonk still has it -> reconcile re-creates the subentry
    await hub.runtime_data.async_refresh()
    await hass.async_block_till_done()
    assert any(s.unique_id == "weather" for s in hub.subentries.values())


async def test_out_of_band_removal_does_not_phantom_delete(hass, byonk, caplog):
    hub = await _hub(hass, byonk)
    assert any(s.unique_id == "weather" for s in hub.subentries.values())

    # byonk drops the package out-of-band; reconcile will remove the subentry itself.
    byonk.packages = []
    with caplog.at_level(logging.WARNING):
        await hub.runtime_data.async_refresh()
        await hass.async_block_till_done()

    # subentry gone (reconcile removed it) ...
    assert not any(s.unique_id == "weather" for s in hub.subentries.values())
    # ... but NO phantom DELETE to byonk, and NO misleading "failed" warning.
    assert byonk.delete_package.await_count == 0
    assert "delete package" not in caplog.text.lower()
