from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from custom_components.byonk import addon


@pytest.fixture
def supervisor(hass):
    """Patch get_supervisor_client with a fake store/addons surface."""
    client = MagicMock()
    client.store.addons_list = AsyncMock(return_value=[])
    client.store.add_repository = AsyncMock()
    client.store.install_addon = AsyncMock()
    client.addons.start_addon = AsyncMock()
    with patch.object(addon, "get_supervisor_client", return_value=client):
        yield client


async def test_find_slug_matches_byonk_config_slug(hass, supervisor):
    item = MagicMock(slug="abcd1234_byonk", name="Byonk",
                     repository="abcd1234", installed=True)
    supervisor.store.addons_list.return_value = [item]
    assert await addon.async_find_addon_slug(hass) == "abcd1234_byonk"


async def test_ensure_adds_repo_when_missing(hass, supervisor):
    # First list empty -> add repo -> second list returns the addon
    item = MagicMock(slug="abcd1234_byonk", name="Byonk",
                     repository="abcd1234", installed=False)
    supervisor.store.addons_list.side_effect = [[], [item]]
    with patch.object(addon, "_async_start", new=AsyncMock()):
        slug = await addon.async_ensure_addon_installed(hass)
    assert slug == "abcd1234_byonk"
    supervisor.store.add_repository.assert_awaited_once()
    supervisor.store.install_addon.assert_awaited_once_with("abcd1234_byonk")


async def test_provision_sets_options_and_restarts(hass):
    mgr = MagicMock()
    mgr.async_get_addon_info = AsyncMock(
        return_value=MagicMock(options={"log_level": "info"})
    )
    mgr.async_set_addon_options = AsyncMock()
    mgr.async_restart_addon = AsyncMock()
    with patch.object(addon, "_get_addon_manager", return_value=mgr):
        token = await addon.async_provision_token(hass, "abcd1234_byonk")
    assert token  # non-empty, generated
    sent = mgr.async_set_addon_options.await_args.args[0]
    assert sent["admin_token"] == token
    assert sent["log_level"] == "info"  # preserves existing options
    mgr.async_restart_addon.assert_awaited_once()
