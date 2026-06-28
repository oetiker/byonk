"""The Byonk integration."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.aiohttp_client import async_get_clientsession

from .addon import async_read_token
from .api import ByonkClient
from .const import CONF_ADDON_SLUG, CONF_BASE_URL, PLATFORMS
from .coordinator import ByonkConfigEntry, ByonkCoordinator


async def async_setup_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    slug = entry.data[CONF_ADDON_SLUG]
    token = await async_read_token(hass, slug)
    if not token:
        raise ConfigEntryAuthFailed("byonk admin token not provisioned")
    client = ByonkClient(
        async_get_clientsession(hass), entry.data[CONF_BASE_URL], token
    )
    coordinator = ByonkCoordinator(hass, entry, client, slug)
    await coordinator.async_config_entry_first_refresh()
    entry.runtime_data = coordinator
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ByonkConfigEntry) -> bool:
    return await hass.config_entries.async_unload_platforms(entry, PLATFORMS)
