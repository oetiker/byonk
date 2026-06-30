"""Byonk number entities."""
from __future__ import annotations

from homeassistant.components.number import NumberEntity
from homeassistant.const import UnitOfTime
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        async_add_entities([ByonkRefreshNumber(coordinator, entry.data[CONF_DEVICE_KEY])])


class ByonkRefreshNumber(ByonkDeviceEntity, NumberEntity):
    # No entity_category: this is a primary control, so it sits in the device's
    # "Controls" card alongside the screen/dither/panel selects.
    _attr_translation_key = "refresh"
    _attr_native_min_value = 0
    _attr_native_max_value = 86400
    _attr_native_step = 60
    _attr_native_unit_of_measurement = UnitOfTime.SECONDS

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_refresh"

    @property
    def native_value(self) -> float | None:
        device = self.device
        # 0 = no override (rather than "unknown").
        return float(device.get("refresh") or 0) if device else None

    async def async_set_native_value(self, value: float) -> None:
        await self.coordinator.client.async_update_device(self._key, {"refresh": int(value)})
        await self.coordinator.async_request_refresh()
