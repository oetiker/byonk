"""Byonk switch entities — global settings and per-device preview options."""
from __future__ import annotations

from typing import Any

from homeassistant.components.switch import SwitchEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import (
    CONF_DEVICE_KEY,
    OPT_PREVIEW_DITHER,
    OPT_PREVIEW_MEASURED,
)
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity, ByonkHubEntity
from .param_entities import ByonkParamSwitch, setup_param_platform


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            [
                ByonkPreviewDitherSwitch(entry.runtime_data, key, entry),
                ByonkPreviewMeasuredSwitch(entry.runtime_data, key, entry),
            ]
        )
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


class _ByonkPreviewOption(ByonkDeviceEntity, SwitchEntity):
    """A view option for this device's screen preview.

    The state lives in the device config entry's `options`, not in byonk: these
    change how the preview is *drawn*, never what the panel shows, so writing
    them to byonk's device configuration would be actively wrong — it would
    alter the real screen to change a picture of it.

    Home Assistant is not asked to reload the entry on a write; the integration
    registers no options-update listener, and the camera reads the current
    value on its next frame.
    """

    _attr_entity_category = EntityCategory.CONFIG
    _option: str

    def __init__(
        self, coordinator: ByonkCoordinator, key: str, entry: ByonkConfigEntry
    ) -> None:
        super().__init__(coordinator, key)
        self._entry = entry
        self._attr_unique_id = f"{key}_{self._option}"
        self._attr_translation_key = self._option

    @property
    def is_on(self) -> bool:
        # Absent means "never touched", and byonk's normal render is the
        # default for both options.
        return bool(self._entry.options.get(self._option, True))

    def _write(self, value: bool) -> None:
        self.hass.config_entries.async_update_entry(
            self._entry, options={**self._entry.options, self._option: value}
        )
        self.async_write_ha_state()

    async def async_turn_on(self, **kwargs: Any) -> None:
        self._write(True)

    async def async_turn_off(self, **kwargs: Any) -> None:
        self._write(False)


class ByonkPreviewDitherSwitch(_ByonkPreviewOption):
    """On: the dithered image the panel receives. Off: the screen before
    dithering — a full-colour rasterization with no palette restriction, which
    is the readable version when you are checking a layout rather than how it
    will reproduce."""

    _option = OPT_PREVIEW_DITHER


class ByonkPreviewMeasuredSwitch(_ByonkPreviewOption):
    """On: the palette drawn in the measured colors a calibration says the
    panel really produces. Off: the spec colors byonk sends to it.

    Has no effect while dithering is off, since an undithered render has no
    palette to map.
    """

    _option = OPT_PREVIEW_MEASURED
