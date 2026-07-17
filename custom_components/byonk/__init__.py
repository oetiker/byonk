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
    DEFAULT_DEVICE_KEY,
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
    # Reload dependent device entries so they (re)bind to *this* coordinator:
    #  - SETUP_RETRY: they loaded before the hub and raised ConfigEntryNotReady.
    #  - LOADED: the hub itself just reloaded (e.g. after a re-auth token
    #    re-provision), which built a brand-new coordinator; a device entry that
    #    stayed LOADED still points at the old, now-dead coordinator via its
    #    runtime_data and would be stuck "unavailable" forever without a reload.
    for dev in hass.config_entries.async_entries(DOMAIN):
        if (
            CONF_DEVICE_KEY in dev.data
            and dev.data.get(CONF_HUB_ENTRY_ID) == entry.entry_id
            and dev.state in (ConfigEntryState.SETUP_RETRY, ConfigEntryState.LOADED)
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
    if entry.data.get(CONF_DEVICE_KEY) == DEFAULT_DEVICE_KEY:
        # The DEFAULT device is byonk-managed and reserved: a manual delete of
        # this config entry must not delete it from byonk (byonk itself
        # rejects that with 409). Leaving it in place lets the next
        # coordinator refresh re-provision the entry via
        # _async_provision_default; deleting it would report DEFAULT as
        # missing from GET /devices forever, since byonk never lets it go.
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
