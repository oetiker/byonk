"""Supervisor add-on lifecycle + token provisioning for byonk."""
from __future__ import annotations

import logging
import secrets

from homeassistant.components.hassio import (
    AddonError,
    AddonManager,
    AddonState,
    get_supervisor_client,
)
from homeassistant.core import HomeAssistant, callback

from .const import ADDON_CONFIG_SLUG, ADDON_NAME, BYONK_ADDON_REPO_URL, DEFAULT_PORT

_LOGGER = logging.getLogger(__name__)


@callback
def _get_addon_manager(hass: HomeAssistant, slug: str) -> AddonManager:
    return AddonManager(hass, _LOGGER, ADDON_NAME, slug)


async def _async_find_addon_item(hass: HomeAssistant):
    """Return the store item for the byonk add-on, or None."""
    client = get_supervisor_client(hass)
    for item in await client.store.addons_list():
        if item.slug.endswith(f"_{ADDON_CONFIG_SLUG}") or item.slug == ADDON_CONFIG_SLUG:
            return item
    return None


async def async_find_addon_slug(hass: HomeAssistant) -> str | None:
    """Return the installable slug of the byonk add-on, or None."""
    item = await _async_find_addon_item(hass)
    return item.slug if item is not None else None


async def async_ensure_addon_installed(hass: HomeAssistant) -> str:
    """Add the repo (if needed), install + start the add-on; return its slug."""
    client = get_supervisor_client(hass)
    item = await _async_find_addon_item(hass)
    if item is None:
        try:
            from aiohasupervisor.models import StoreAddRepository

            await client.store.add_repository(
                StoreAddRepository(repository=BYONK_ADDON_REPO_URL)
            )
        except Exception as err:  # SupervisorError subclasses
            raise AddonError(f"Could not add byonk add-on repository: {err}") from err
        item = await _async_find_addon_item(hass)
        if item is None:
            raise AddonError("byonk add-on not found after adding repository")

    slug = item.slug
    # Install if needed.
    if not getattr(item, "installed", False):
        await client.store.install_addon(slug)
    await _async_start(hass, slug)
    return slug


async def _async_start(hass: HomeAssistant, slug: str) -> None:
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    if info.state != AddonState.RUNNING:
        await mgr.async_start_addon()


async def async_provision_token(hass: HomeAssistant, slug: str) -> str:
    """Generate a token, merge into add-on options, restart; return the token."""
    mgr = _get_addon_manager(hass, slug)
    token = secrets.token_hex(32)
    info = await mgr.async_get_addon_info()
    options = dict(info.options or {})
    options["admin_token"] = token
    await mgr.async_set_addon_options(options)
    await mgr.async_restart_addon()
    return token


async def async_read_token(hass: HomeAssistant, slug: str) -> str | None:
    """Read the admin token back from the add-on option (single source of truth)."""
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    token = (info.options or {}).get("admin_token")
    return token or None


async def async_get_base_url(hass: HomeAssistant, slug: str) -> str:
    mgr = _get_addon_manager(hass, slug)
    info = await mgr.async_get_addon_info()
    return f"http://{info.hostname}:{DEFAULT_PORT}"
