"""Dynamic per-package status sensors on the hub device."""
from __future__ import annotations

from homeassistant.components.sensor import SensorEntity
from homeassistant.const import EntityCategory
from homeassistant.core import callback
from homeassistant.helpers import entity_registry as er
from homeassistant.util import dt as dt_util

from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkHubEntity


class ByonkPackageStatusSensor(ByonkHubEntity, SensorEntity):
    """One sensor per non-builtin package: state = fetch status."""

    _attr_entity_category = EntityCategory.DIAGNOSTIC
    _attr_translation_key = "package_status"

    def __init__(self, coordinator: ByonkCoordinator, handle: str) -> None:
        super().__init__(coordinator)
        self._handle = handle
        self._attr_unique_id = f"{coordinator.entry.entry_id}_pkg_{handle}_status"
        self._attr_translation_placeholders = {"handle": handle}
        self._attr_name = f"{handle}: status"

    @property
    def _pkg(self) -> dict | None:
        return self.coordinator.data.package(self._handle)

    @property
    def available(self) -> bool:
        return super().available and self._pkg is not None

    @property
    def native_value(self) -> str | None:
        pkg = self._pkg
        return pkg.get("status") if pkg else None

    @property
    def extra_state_attributes(self) -> dict:
        pkg = self._pkg or {}
        lf = pkg.get("last_fetched")
        return {
            "resolved_sha": pkg.get("resolved_sha"),
            "last_fetched": dt_util.parse_datetime(lf) if lf else None,
            "error": pkg.get("error"),
            "repo": pkg.get("repo"),
            "pin": pkg.get("pin"),
            "pin_kind": pkg.get("pin_kind"),
        }


class PackageStatusManager:
    """Add/remove a status sensor per non-builtin package as the registry changes."""

    def __init__(self, coordinator: ByonkCoordinator, async_add_entities) -> None:
        self._coordinator = coordinator
        self._async_add_entities = async_add_entities
        self._entities: dict[str, ByonkPackageStatusSensor] = {}

    @callback
    def reconcile(self) -> None:
        desired = {p["handle"] for p in self._coordinator.data.non_builtin_packages()}
        new = {
            h: ByonkPackageStatusSensor(self._coordinator, h)
            for h in desired
            if h not in self._entities
        }
        for h, ent in new.items():
            self._entities[h] = ent
        if new:
            self._async_add_entities(list(new.values()))
        for h in list(self._entities):
            if h not in desired:
                self._remove(self._entities.pop(h))

    def _remove(self, entity: ByonkPackageStatusSensor) -> None:
        registry = er.async_get(self._coordinator.hass)
        if entity.entity_id and registry.async_get(entity.entity_id):
            registry.async_remove(entity.entity_id)
        else:
            self._coordinator.hass.async_create_task(
                entity.async_remove(force_remove=True)
            )


def setup_package_status_platform(entry: ByonkConfigEntry, async_add_entities) -> None:
    coordinator = entry.runtime_data
    manager = PackageStatusManager(coordinator, async_add_entities)
    manager.reconcile()
    entry.async_on_unload(coordinator.async_add_listener(manager.reconcile))
