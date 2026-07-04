"""Byonk buttons (hub actions)."""
from __future__ import annotations

import logging

from homeassistant.components.button import ButtonEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .entity import ByonkHubEntity

_LOGGER = logging.getLogger(__name__)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        return  # device entries have no hub buttons
    async_add_entities([ByonkUpdatePackagesButton(entry.runtime_data)])


class ByonkUpdatePackagesButton(ByonkHubEntity, ButtonEntity):
    _attr_translation_key = "update_packages"

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_update_packages"

    async def async_press(self) -> None:
        try:
            await self.coordinator.client.async_update_packages()
        except ByonkApiError as err:
            _LOGGER.warning("update packages failed: %s", err)
            return
        await self.coordinator.async_request_refresh()
