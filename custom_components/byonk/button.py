"""Byonk buttons — hub actions and per-device actions."""
from __future__ import annotations

import logging

from homeassistant.components.button import ButtonEntity
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY, OPT_PREVIEW_DITHER, OPT_PREVIEW_MEASURED
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity, ByonkHubEntity

_LOGGER = logging.getLogger(__name__)


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        async_add_entities(
            [
                ByonkRefreshPreviewButton(
                    entry.runtime_data, entry.data[CONF_DEVICE_KEY], entry
                )
            ]
        )
        return
    async_add_entities([ByonkUpdateScreenReposButton(entry.runtime_data)])


class ByonkUpdateScreenReposButton(ByonkHubEntity, ButtonEntity):
    _attr_translation_key = "update_screen_repos"

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_update_screen_repos"

    async def async_press(self) -> None:
        try:
            await self.coordinator.client.async_update_screen_repos()
        except ByonkApiError as err:
            _LOGGER.warning("update screen repos failed: %s", err)
            return
        await self.coordinator.async_request_refresh()


class ByonkRefreshPreviewButton(ByonkDeviceEntity, ButtonEntity):
    """Re-render the screen preview now.

    byonk holds a rendered preview until the device's configuration changes or
    the screen's own refresh rate elapses. That is right for a screen whose
    content follows its configuration, but a screen whose *data* moves on its
    own — a clock, a weather forecast — can sit still in the preview while the
    panel has moved on. This forces the render.

    Nothing is invalidated on this side: the forced render replaces byonk's
    cached copy, so the camera's next frame picks it up.
    """

    _attr_translation_key = "refresh_preview"

    def __init__(
        self, coordinator: ByonkCoordinator, key: str, entry: ByonkConfigEntry
    ) -> None:
        super().__init__(coordinator, key)
        self._entry = entry
        self._attr_unique_id = f"{key}_refresh_preview"

    async def async_press(self) -> None:
        # Re-render the variant currently on display, not byonk's default one,
        # or pressing refresh while dithering is off would refresh an image
        # nobody is looking at.
        try:
            await self.coordinator.client.async_get_device_preview(
                self._key,
                force=True,
                dither=self._entry.options.get(OPT_PREVIEW_DITHER, True),
                measured=self._entry.options.get(OPT_PREVIEW_MEASURED, True),
            )
        except ByonkApiError as err:
            _LOGGER.warning("refresh preview for %s failed: %s", self._key, err)
