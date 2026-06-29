"""Byonk select entities."""
from __future__ import annotations

from homeassistant.components.select import SelectEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity
from .param_form import default_params


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    for sub_id, sub in entry.subentries.items():
        if sub.subentry_type != "device":
            continue
        key = sub.data["key"]
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ],
            config_subentry_id=sub_id,
        )


class _ByonkSelect(ByonkDeviceEntity, SelectEntity):
    _field: str

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_{self._field}"
        self._attr_translation_key = self._field

    @property
    def current_option(self) -> str | None:
        device = self.device
        return device.get(self._field) if device else None

    async def _write(self, payload: dict) -> None:
        await self.coordinator.client.async_update_device(self._key, payload)
        await self.coordinator.async_request_refresh()


class ByonkScreenSelect(_ByonkSelect):
    _field = "screen"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.screen_names()

    async def async_select_option(self, option: str) -> None:
        params = default_params(self.coordinator.data.screen_params(option))
        await self._write({"screen": option, "params": params})


class ByonkDitherSelect(_ByonkSelect):
    _field = "dither"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.dither

    async def async_select_option(self, option: str) -> None:
        await self._write({"dither": option})


class ByonkPanelSelect(_ByonkSelect):
    _field = "panel"

    @property
    def options(self) -> list[str]:
        return self.coordinator.data.panel_names()

    async def async_select_option(self, option: str) -> None:
        await self._write({"panel": option})
