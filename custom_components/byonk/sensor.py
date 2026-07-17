"""Byonk sensors."""
from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass

from homeassistant.components.sensor import (
    SensorDeviceClass,
    SensorEntity,
    SensorEntityDescription,
)
from homeassistant.const import EntityCategory, UnitOfElectricPotential
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback
from homeassistant.util import dt as dt_util

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .entity import ByonkDeviceEntity
from .screen_repo_entities import setup_screen_repo_status_platform


@dataclass(frozen=True, kw_only=True)
class ByonkSensorDesc(SensorEntityDescription):
    value: Callable[[dict], object]


DEVICE_SENSORS: tuple[ByonkSensorDesc, ...] = (
    ByonkSensorDesc(
        key="battery_voltage",
        translation_key="battery_voltage",
        device_class=SensorDeviceClass.VOLTAGE,
        native_unit_of_measurement=UnitOfElectricPotential.VOLT,
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("battery_voltage"),
    ),
    ByonkSensorDesc(
        key="rssi",
        translation_key="rssi",
        device_class=SensorDeviceClass.SIGNAL_STRENGTH,
        native_unit_of_measurement="dBm",
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("rssi"),
    ),
    ByonkSensorDesc(
        key="last_seen",
        translation_key="last_seen",
        device_class=SensorDeviceClass.TIMESTAMP,
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: dt_util.parse_datetime(d["last_seen"]) if d.get("last_seen") else None,
    ),
    ByonkSensorDesc(
        key="firmware_version",
        translation_key="firmware_version",
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("firmware_version"),
    ),
    ByonkSensorDesc(
        key="model",
        translation_key="model",
        entity_category=EntityCategory.DIAGNOSTIC,
        value=lambda d: d.get("model"),
    ),
)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            ByonkDeviceSensor(coordinator, key, desc) for desc in DEVICE_SENSORS
        )
        return
    setup_screen_repo_status_platform(entry, async_add_entities)


class ByonkDeviceSensor(ByonkDeviceEntity, SensorEntity):
    entity_description: ByonkSensorDesc

    def __init__(self, coordinator, key, description: ByonkSensorDesc) -> None:
        super().__init__(coordinator, key)
        self.entity_description = description
        self._attr_unique_id = f"{key}_{description.key}"

    @property
    def native_value(self):
        device = self.device
        return self.entity_description.value(device) if device else None
