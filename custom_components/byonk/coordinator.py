"""Data coordinator for byonk."""
from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import timedelta
import logging

from homeassistant.config_entries import ConfigEntry, ConfigSubentry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import DOMAIN, UPDATE_INTERVAL_SECONDS
from .repairs import async_sync_pending_issues

_LOGGER = logging.getLogger(__name__)

type ByonkConfigEntry = ConfigEntry["ByonkCoordinator"]


@dataclass(frozen=True)
class ByonkData:
    devices: list[dict]
    pending: list[dict]
    screens: list[dict]
    panels: list[dict]
    dither: list[str]
    config: dict

    def screen_names(self) -> list[str]:
        return [s["name"] for s in self.screens]

    def panel_names(self) -> list[str]:
        return [p["name"] for p in self.panels]

    def screen_params(self, name: str) -> list[dict]:
        for s in self.screens:
            if s["name"] == name:
                return s.get("params") or []
        return []

    def default_screen(self) -> str | None:
        return self.config.get("default_screen")

    def registration_enabled(self) -> bool:
        return bool(self.config.get("registration", {}).get("enabled", False))

    def auth_mode(self) -> str | None:
        return self.config.get("auth_mode")


class ByonkCoordinator(DataUpdateCoordinator[ByonkData]):
    def __init__(
        self, hass: HomeAssistant, entry: ByonkConfigEntry, client: ByonkClient, slug: str
    ) -> None:
        super().__init__(
            hass,
            _LOGGER,
            name=DOMAIN,
            update_interval=timedelta(seconds=UPDATE_INTERVAL_SECONDS),
            always_update=False,
        )
        self.client = client
        self.entry = entry
        self.slug = slug
        self._missing_removals: dict[str, int] = {}

    async def _async_update_data(self) -> ByonkData:
        try:
            devices, pending, screens, config = await asyncio.gather(
                self.client.async_get_devices(),
                self.client.async_get_pending(),
                self.client.async_get_screens(),
                self.client.async_get_config(),
            )
        except ByonkAuthError as err:
            raise ConfigEntryAuthFailed(str(err)) from err
        except ByonkApiError as err:
            raise UpdateFailed(str(err)) from err
        data = ByonkData(
            devices=devices,
            pending=pending,
            screens=screens.get("screens", []),
            panels=screens.get("panels", []),
            dither=screens.get("dither_algorithms", []),
            config=config,
        )
        self._async_reconcile(data)
        async_sync_pending_issues(self.hass, data.pending)
        return data

    def _async_reconcile(self, data: ByonkData) -> None:
        existing = {
            sub.unique_id: sub_id
            for sub_id, sub in self.entry.subentries.items()
            if sub.subentry_type == "device"
        }
        registered_keys = {
            d["key"] for d in data.devices if d.get("registered")
        }
        for key in registered_keys - set(existing):
            self.hass.config_entries.async_add_subentry(
                self.entry,
                ConfigSubentry(
                    data={"key": key},
                    subentry_type="device",
                    title=key,
                    unique_id=key,
                ),
            )
        absent = set(existing) - registered_keys
        # reset strike counts for devices that are present
        for key in registered_keys:
            self._missing_removals.pop(key, None)
        for key in absent:
            self._missing_removals[key] = self._missing_removals.get(key, 0) + 1
            if self._missing_removals[key] >= 2:
                self.hass.config_entries.async_remove_subentry(
                    self.entry, existing[key]
                )
                self._missing_removals.pop(key, None)
