"""Dynamic per-screen parameter entities for byonk devices."""
from __future__ import annotations

import logging

from homeassistant.components.text import TextEntity, TextMode
from homeassistant.core import callback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity

_LOGGER = logging.getLogger(__name__)


class ByonkParamEntity(ByonkDeviceEntity):
    """Base for entities editing one screen @param of a device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator, key: str, field: dict) -> None:
        super().__init__(coordinator, key)
        self._field = field
        self._attr_unique_id = f"{key}_param_{field['name']}"
        self._attr_name = field.get("label") or field["name"]

    @property
    def _current_params(self) -> dict:
        device = self.device
        return dict(device.get("params") or {}) if device else {}

    @property
    def _value(self):
        return self._current_params.get(self._field["name"])

    @property
    def available(self) -> bool:
        if not super().available:
            return False
        device = self.device
        if not device:
            return False
        fields = self.coordinator.data.screen_params(device.get("screen"))
        return any(f["name"] == self._field["name"] for f in fields)

    async def _write_param(self, value) -> None:
        async with self.coordinator.param_lock(self._key):
            device = self.device
            params = dict(device.get("params") or {}) if device else {}
            params[self._field["name"]] = value
            try:
                await self.coordinator.client.async_update_device(
                    self._key, {"params": params}
                )
            except ByonkApiError as err:
                _LOGGER.warning(
                    "param write failed for %s.%s: %s",
                    self._key, self._field["name"], err,
                )
                return
            await self.coordinator.async_refresh()


class ByonkParamText(ByonkParamEntity, TextEntity):
    def __init__(self, coordinator: ByonkCoordinator, key: str, field: dict) -> None:
        super().__init__(coordinator, key, field)
        self._attr_mode = (
            TextMode.PASSWORD if field.get("sensitive") else TextMode.TEXT
        )

    @property
    def native_value(self) -> str | None:
        v = self._value
        return None if v is None else str(v)

    async def async_set_value(self, value: str) -> None:
        await self._write_param(value)


class ParamPlatformManager:
    """Add/remove param entities of given types as the device's screen changes."""

    def __init__(self, coordinator, key, async_add_entities, types, entity_cls):
        self._coordinator = coordinator
        self._key = key
        self._async_add_entities = async_add_entities
        self._types = types
        self._entity_cls = entity_cls
        self._entities: dict[str, ByonkParamEntity] = {}

    def _device_screen(self) -> str | None:
        for d in self._coordinator.data.devices:
            if d.get("key") == self._key:
                return d.get("screen")
        return None

    @callback
    def reconcile(self) -> None:
        screen = self._device_screen()
        fields = self._coordinator.data.screen_params(screen) if screen else []
        desired = {
            f["name"]: f
            for f in fields
            if f.get("type") in self._types and not f.get("hidden")
        }
        new = {
            name: self._entity_cls(self._coordinator, self._key, field)
            for name, field in desired.items()
            if name not in self._entities
        }
        for name, entity in new.items():
            self._entities[name] = entity
        if new:
            self._async_add_entities(list(new.values()))
        for name in list(self._entities):
            if name not in desired:
                entity = self._entities.pop(name)
                self._coordinator.hass.async_create_task(
                    entity.async_remove(force_remove=True)
                )


def setup_param_platform(
    entry: ByonkConfigEntry, async_add_entities, types: set[str], entity_cls
) -> None:
    """Wire a platform's param entities for a device entry (dynamic per screen)."""
    coordinator = entry.runtime_data
    key = entry.data[CONF_DEVICE_KEY]
    manager = ParamPlatformManager(
        coordinator, key, async_add_entities, types, entity_cls
    )
    manager.reconcile()
    entry.async_on_unload(coordinator.async_add_listener(manager.reconcile))
