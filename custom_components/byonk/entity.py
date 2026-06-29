"""Base entities for byonk."""
from __future__ import annotations

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import ADDON_NAME, CONF_BASE_URL, DOMAIN
from .coordinator import ByonkCoordinator


class ByonkHubEntity(CoordinatorEntity[ByonkCoordinator]):
    """Entity attached to the Byonk Server hub device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator) -> None:
        super().__init__(coordinator)
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, coordinator.entry.entry_id)},
            name=ADDON_NAME,
            manufacturer="Byonk",
            configuration_url=coordinator.entry.data.get(CONF_BASE_URL),
        )


class ByonkDeviceEntity(CoordinatorEntity[ByonkCoordinator]):
    """Entity attached to one TRMNL device."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator)
        self._key = key
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, key)},
            name=f"TRMNL {key}",
            manufacturer="TRMNL",
            via_device=(DOMAIN, coordinator.entry.entry_id),
        )

    @property
    def device(self) -> dict | None:
        for d in self.coordinator.data.devices:
            if d.get("key") == self._key:
                return d
        return None

    @property
    def available(self) -> bool:
        return super().available and self.device is not None
