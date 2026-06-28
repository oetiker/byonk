"""Base entities for byonk."""
from __future__ import annotations

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import ADDON_NAME, DOMAIN
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
            configuration_url=coordinator.client._base,  # noqa: SLF001
        )
