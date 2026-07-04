"""The Byonk integration."""
from __future__ import annotations

import logging

from homeassistant.config_entries import ConfigEntryState
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed, ConfigEntryNotReady
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .addon import async_read_token
from .api import ByonkApiError, ByonkClient
from .const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
    PLATFORMS,
)
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .name_sync import async_setup_name_sync

_LOGGER = logging.getLogger(__name__)


async def async_setup_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    if CONF_DEVICE_KEY in entry.data:
        return await _async_setup_device_entry(hass, entry)
    return await _async_setup_hub_entry(hass, entry)


async def _async_setup_hub_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    slug = entry.data[CONF_ADDON_SLUG]
    token = await async_read_token(hass, slug)
    if not token:
        raise ConfigEntryAuthFailed("byonk admin token not provisioned")
    client = ByonkClient(async_get_clientsession(hass), entry.data[CONF_BASE_URL], token)
    coordinator = ByonkCoordinator(hass, entry, client, slug)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    hass.data.setdefault(DOMAIN, {})[entry.entry_id] = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    # Device entries that loaded before the hub raised ConfigEntryNotReady; nudge them.
    for dev in hass.config_entries.async_entries(DOMAIN):
        if (
            CONF_DEVICE_KEY in dev.data
            and dev.data.get(CONF_HUB_ENTRY_ID) == entry.entry_id
            and dev.state is ConfigEntryState.SETUP_RETRY
        ):
            hass.async_create_task(hass.config_entries.async_reload(dev.entry_id))
    return True


async def _async_setup_device_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    hub_id = entry.data[CONF_HUB_ENTRY_ID]
    coordinator = hass.data.get(DOMAIN, {}).get(hub_id)
    if coordinator is None:
        raise ConfigEntryNotReady("byonk hub not ready")
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    await async_setup_name_sync(hass, entry, coordinator)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    unloaded = await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
    if unloaded and CONF_DEVICE_KEY not in entry.data:
        hass.data.get(DOMAIN, {}).pop(entry.entry_id, None)
    return unloaded


async def async_remove_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> None:
    """When a device entry is removed, delete its mapping from byonk (best-effort)."""
    if CONF_DEVICE_KEY not in entry.data:
        return
    hub = hass.config_entries.async_get_entry(entry.data[CONF_HUB_ENTRY_ID])
    if hub is None:
        return
    token = await async_read_token(hass, hub.data[CONF_ADDON_SLUG])
    if not token:
        return
    client = ByonkClient(
        async_get_clientsession(hass), hub.data[CONF_BASE_URL], token
    )
    try:
        await client.async_delete_device(entry.data[CONF_DEVICE_KEY])
    except ByonkApiError:
        pass
