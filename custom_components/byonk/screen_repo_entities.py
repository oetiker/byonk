"""Dynamic per-screen-repo status sensors on the hub device."""
from __future__ import annotations

from homeassistant.components.sensor import SensorEntity
from homeassistant.const import EntityCategory
from homeassistant.core import callback
from homeassistant.helpers import entity_registry as er
from homeassistant.util import dt as dt_util

from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkHubEntity


class ByonkScreenRepoStatusSensor(ByonkHubEntity, SensorEntity):
    """One sensor per non-builtin screen repo: state = fetch status."""

    _attr_entity_category = EntityCategory.DIAGNOSTIC
    _attr_translation_key = "screen_repo_status"

    def __init__(self, coordinator: ByonkCoordinator, handle: str) -> None:
        super().__init__(coordinator)
        self._handle = handle
        self._attr_unique_id = f"{coordinator.entry.entry_id}_repo_{handle}_status"
        self._attr_translation_placeholders = {"handle": handle}
        self._attr_name = f"{handle}: status"

    @property
    def _repo(self) -> dict | None:
        return self.coordinator.data.screen_repo(self._handle)

    @property
    def available(self) -> bool:
        return super().available and self._repo is not None

    @property
    def native_value(self) -> str | None:
        repo = self._repo
        return repo.get("status") if repo else None

    @property
    def extra_state_attributes(self) -> dict:
        repo = self._repo or {}
        lf = repo.get("last_fetched")
        return {
            "resolved_sha": repo.get("resolved_sha"),
            "last_fetched": dt_util.parse_datetime(lf) if lf else None,
            "error": repo.get("error"),
            "repo": repo.get("repo"),
            "pin": repo.get("pin"),
            "pin_kind": repo.get("pin_kind"),
        }


class ScreenRepoStatusManager:
    """Add/remove a status sensor per non-builtin screen repo as the registry changes."""

    def __init__(self, coordinator: ByonkCoordinator, async_add_entities) -> None:
        self._coordinator = coordinator
        self._async_add_entities = async_add_entities
        self._entities: dict[str, ByonkScreenRepoStatusSensor] = {}

    @callback
    def reconcile(self) -> None:
        desired = {r["handle"] for r in self._coordinator.data.non_builtin_screen_repos()}
        new = {
            h: ByonkScreenRepoStatusSensor(self._coordinator, h)
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

    def _remove(self, entity: ByonkScreenRepoStatusSensor) -> None:
        registry = er.async_get(self._coordinator.hass)
        if entity.entity_id and registry.async_get(entity.entity_id):
            registry.async_remove(entity.entity_id)
        else:
            self._coordinator.hass.async_create_task(
                entity.async_remove(force_remove=True)
            )


def setup_screen_repo_status_platform(entry: ByonkConfigEntry, async_add_entities) -> None:
    coordinator = entry.runtime_data
    manager = ScreenRepoStatusManager(coordinator, async_add_entities)
    manager.reconcile()
    entry.async_on_unload(coordinator.async_add_listener(manager.reconcile))
