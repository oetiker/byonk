"""Byonk text entities: the per-device refresh override + string/number params."""
from __future__ import annotations

import logging

from homeassistant.components.text import TextEntity, TextMode
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity
from .param_entities import ByonkParamText, setup_param_platform

_LOGGER = logging.getLogger(__name__)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        async_add_entities(
            [ByonkRefreshText(entry.runtime_data, entry.data[CONF_DEVICE_KEY])]
        )
        # int/float params are rendered as text fields too (label-above), coerced
        # back to numbers on write — see ByonkParamText.
        setup_param_platform(
            entry,
            async_add_entities,
            {"string", "color", "url", "int", "float"},
            ByonkParamText,
        )


class ByonkRefreshText(ByonkDeviceEntity, TextEntity):
    """Per-device refresh-interval override (seconds); 0 = no override.

    A text field (not a Number) so it renders label-above like the other device
    controls; the value is parsed to a non-negative int on write.
    """

    _attr_translation_key = "refresh"
    _attr_mode = TextMode.TEXT
    # No entity_category: a primary control, sits in the device's "Controls" card.

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_refresh"

    @property
    def native_value(self) -> str | None:
        device = self.device
        return None if device is None else str(device.get("refresh") or 0)

    async def async_set_value(self, value: str) -> None:
        try:
            seconds = int(float(value))
        except (TypeError, ValueError):
            _LOGGER.warning("refresh for %s: %r is not a number", self._key, value)
            return
        if seconds < 0:
            seconds = 0
        try:
            await self.coordinator.client.async_update_device(
                self._key, {"refresh": seconds}
            )
        except ByonkApiError as err:
            _LOGGER.warning("refresh write failed for %s: %s", self._key, err)
            return
        await self.coordinator.async_request_refresh()
