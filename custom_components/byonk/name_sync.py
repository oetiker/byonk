"""Mirror a device's Home Assistant name down to byonk (one-way, HA -> byonk)."""
from __future__ import annotations

import logging

from homeassistant.core import Event, HomeAssistant, callback
from homeassistant.helpers import device_registry as dr
from homeassistant.helpers.event import async_track_device_registry_updated_event

from .api import ByonkApiError
from .const import CONF_DEVICE_KEY, DOMAIN
from .coordinator import ByonkConfigEntry, ByonkCoordinator

_LOGGER = logging.getLogger(__name__)


def _effective_name(device: dr.DeviceEntry | None) -> str:
    """The deliberately-chosen name only; '' means no user name (identify by MAC)."""
    if device is None:
        return ""
    return device.name_by_user or ""


async def async_setup_name_sync(
    hass: HomeAssistant, entry: ByonkConfigEntry, coordinator: ByonkCoordinator
) -> None:
    key = entry.data[CONF_DEVICE_KEY]
    registry = dr.async_get(hass)
    device = registry.async_get_device(identifiers={(DOMAIN, key)})
    if device is None:
        # Entities create the device during platform setup; if it is somehow not
        # present yet, skip — nothing to track or seed.
        return

    async def _push(name: str) -> None:
        try:
            await coordinator.client.async_update_device(key, {"name": name})
        except ByonkApiError as err:
            _LOGGER.warning("name sync failed for %s: %s", key, err)
            return
        await coordinator.async_request_refresh()

    # Seed once if byonk's stored name differs from HA's chosen name.
    desired = _effective_name(device)
    current = ""
    for d in coordinator.data.devices:
        if d.get("key") == key:
            current = d.get("name") or ""
            break
    if desired != current:
        await _push(desired)

    @callback
    def _handle_update(event: Event) -> None:
        if event.data.get("action") != "update":
            return
        if "name_by_user" not in event.data.get("changes", {}):
            return
        updated = registry.async_get_device(identifiers={(DOMAIN, key)})
        hass.async_create_task(_push(_effective_name(updated)))

    entry.async_on_unload(
        async_track_device_registry_updated_event(hass, device.id, _handle_update)
    )
