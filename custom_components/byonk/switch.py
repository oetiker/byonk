"""Byonk switch entities (global settings)."""
from __future__ import annotations

from typing import Any

from homeassistant.components.switch import SwitchEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .entity import ByonkHubEntity
from .param_entities import ByonkParamSwitch, setup_param_platform


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        setup_param_platform(entry, async_add_entities, {"bool"}, ByonkParamSwitch)
        return
    async_add_entities([ByonkRegistrationSwitch(entry.runtime_data)])


class ByonkRegistrationSwitch(ByonkHubEntity, SwitchEntity):
    _attr_translation_key = "registration_enabled"
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_registration_enabled"

    @property
    def is_on(self) -> bool:
        return self.coordinator.data.registration_enabled()

    async def async_turn_on(self, **kwargs: Any) -> None:
        await self.coordinator.client.async_update_settings({"registration_enabled": True})
        await self.coordinator.async_request_refresh()

    async def async_turn_off(self, **kwargs: Any) -> None:
        await self.coordinator.client.async_update_settings({"registration_enabled": False})
        await self.coordinator.async_request_refresh()
