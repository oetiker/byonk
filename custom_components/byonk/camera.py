"""Byonk screen preview — what a device's panel is showing.

A camera rather than an image entity, because Home Assistant renders a camera
on the device page as a full-width picture; an image entity appears only as a
row thumbnail, which is too small to judge a screen by.

Home Assistant never polls a camera (`Camera._attr_should_poll` is `False`).
Frames are pulled only while a browser holds the picture open, so a device
nobody is looking at costs nothing. While somebody *is* looking, the frontend
pulls one frame per `frame_interval`; the default of 0.5 s is meant for real
video and would be one full screen render twice a second, so it is widened
here. The renders themselves are deduplicated by byonk, which serves a cached
PNG until the device's configuration changes or the screen's own refresh rate
elapses — so this entity keeps no second cache of its own.
"""
from __future__ import annotations

import logging

from homeassistant.components.camera import Camera
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY, OPT_PREVIEW_DITHER, OPT_PREVIEW_MEASURED
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity

_LOGGER = logging.getLogger(__name__)

# Seconds between frame pulls while somebody has the device page open. byonk
# answers most of these from its cache, so this is about how quickly a
# configuration change becomes visible, not about render cost.
FRAME_INTERVAL = 10.0


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY not in entry.data:
        return  # the hub has no screen of its own to preview
    async_add_entities(
        [ByonkScreenPreview(entry.runtime_data, entry.data[CONF_DEVICE_KEY], entry)]
    )


class ByonkScreenPreview(ByonkDeviceEntity, Camera):
    """The device's current screen, rendered by byonk."""

    _attr_translation_key = "screen_preview"
    _attr_frame_interval = FRAME_INTERVAL

    def __init__(
        self, coordinator: ByonkCoordinator, key: str, entry: ByonkConfigEntry
    ) -> None:
        # Neither base calls `super().__init__()`, so both are initialised
        # explicitly. Order matters only in that `Camera.__init__` resets
        # `content_type`, which is corrected immediately below.
        ByonkDeviceEntity.__init__(self, coordinator, key)
        Camera.__init__(self)
        # byonk renders PNG. `Camera.__init__` defaults this to `image/jpeg`,
        # which would both mislabel the response and send Home Assistant down
        # its JPEG-scaling path (`_async_get_image` picks it purely by
        # content type) with bytes libturbojpeg cannot read.
        self.content_type = "image/png"
        self._entry = entry
        self._attr_unique_id = f"{key}_screen_preview"

    async def async_camera_image(
        self, width: int | None = None, height: int | None = None
    ) -> bytes | None:
        """Fetch the rendered screen.

        `width`/`height` are ignored: byonk renders at the panel's own
        resolution and cannot produce another one, and scaling an e-ink dither
        pattern would destroy the very texture the preview exists to show.
        Home Assistant treats them as a hint, so returning the native size is
        allowed.
        """
        try:
            return await self.coordinator.client.async_get_device_preview(
                self._key,
                dither=self._entry.options.get(OPT_PREVIEW_DITHER, True),
                measured=self._entry.options.get(OPT_PREVIEW_MEASURED, True),
            )
        except ByonkApiError as err:
            # Never raise: a failed frame would surface as an unavailable
            # camera and hide the rest of the device page's controls.
            _LOGGER.debug("screen preview for %s failed: %s", self._key, err)
            return None
