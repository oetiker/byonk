"""Data coordinator for byonk."""
from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import timedelta
import logging

from homeassistant.config_entries import (
    SOURCE_INTEGRATION_DISCOVERY,
    ConfigEntry,
)
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import CONF_DEVICE_KEY, DEFAULT_DEVICE_KEY, DOMAIN, UPDATE_INTERVAL_SECONDS

_LOGGER = logging.getLogger(__name__)

type ByonkConfigEntry = ConfigEntry["ByonkCoordinator"]

REMOVE_STRIKES = 2


@dataclass(frozen=True)
class ByonkData:
    devices: list[dict]
    pending: list[dict]
    screens: list[dict]
    panels: list[dict]
    dither: list[str]
    config: dict
    packages: list[dict]

    def screen_names(self) -> list[str]:
        # Screens are addressed by their qualified `handle/path` ref.
        return [s["ref"] for s in self.screens]

    def panel_names(self) -> list[str]:
        return [p["name"] for p in self.panels]

    def screen_params(self, ref: str) -> list[dict]:
        for s in self.screens:
            if s["ref"] == ref:
                return s.get("params") or []
        return []

    def registration_enabled(self) -> bool:
        return bool(self.config.get("registration", {}).get("enabled", False))

    def auth_mode(self) -> str | None:
        return self.config.get("auth_mode")

    def non_builtin_packages(self) -> list[dict]:
        return [p for p in self.packages if not p.get("builtin")]

    def package(self, handle: str) -> dict | None:
        for p in self.packages:
            if p.get("handle") == handle:
                return p
        return None


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
        self._remove_strikes: dict[str, int] = {}
        self._orphan_strikes: dict[str, int] = {}

    async def _async_update_data(self) -> ByonkData:
        try:
            devices, pending, screens, config, packages = await asyncio.gather(
                self.client.async_get_devices(),
                self.client.async_get_pending(),
                self.client.async_get_screens(),
                self.client.async_get_config(),
                self.client.async_get_packages(),
            )
        except ByonkAuthError as err:
            raise ConfigEntryAuthFailed(str(err)) from err
        except ByonkApiError as err:
            raise UpdateFailed(str(err)) from err
        # The admin API now groups screens by package: {packages: [{screens: [...]}]}.
        # Flatten to a single list; each screen carries its qualified `ref`.
        flat_screens = [
            screen
            for pkg in screens.get("packages", [])
            for screen in pkg.get("screens", [])
        ]
        data = ByonkData(
            devices=devices,
            pending=pending,
            screens=flat_screens,
            panels=screens.get("panels", []),
            dither=screens.get("dither_algorithms", []),
            config=config,
            packages=packages,
        )
        # Skip device reconcile on the very first refresh: entry.runtime_data is not
        # yet set at that point (it is set in __init__.py after
        # async_config_entry_first_refresh returns), so HA device-entry lookups and
        # eager task execution would race against the incomplete setup. Discovery
        # sync is still scheduled with eager_start=False so it runs on the first real
        # event-loop yield AFTER runtime_data is set.
        if self.data is not None:
            await self._async_reconcile(data)
        self._async_sync_discovery(data)
        self._async_provision_default(data)
        return data

    def _device_entries(self) -> dict[str, ConfigEntry]:
        return {
            e.data[CONF_DEVICE_KEY]: e
            for e in self.hass.config_entries.async_entries(DOMAIN)
            if CONF_DEVICE_KEY in e.data
        }

    async def _async_reconcile(self, data: ByonkData) -> None:
        device_entries = self._device_entries()
        ha_keys = set(device_entries) - {DEFAULT_DEVICE_KEY}  # never auto-remove
        byonk_registered = {d["key"] for d in data.devices if d.get("registered")}
        byonk_registered.discard(DEFAULT_DEVICE_KEY)  # reserved: never orphan-prune

        for key in ha_keys & byonk_registered:
            self._remove_strikes.pop(key, None)
            self._orphan_strikes.pop(key, None)

        # HA entry exists, byonk no longer registers it -> remove HA entry (grace).
        for key in ha_keys - byonk_registered:
            self._remove_strikes[key] = self._remove_strikes.get(key, 0) + 1
            if self._remove_strikes[key] >= REMOVE_STRIKES:
                self._remove_strikes.pop(key, None)
                self.hass.async_create_task(
                    self.hass.config_entries.async_remove(device_entries[key].entry_id)
                )

        # byonk registers a device HA has no entry for -> orphan; delete from byonk (grace).
        for key in byonk_registered - ha_keys:
            self._orphan_strikes[key] = self._orphan_strikes.get(key, 0) + 1
            if self._orphan_strikes[key] >= REMOVE_STRIKES:
                self._orphan_strikes.pop(key, None)
                try:
                    await self.client.async_delete_device(key)
                except ByonkApiError as err:
                    _LOGGER.warning("orphan prune failed for %s: %s", key, err)

    def _async_provision_default(self, data: ByonkData) -> None:
        has_default = any(
            d.get("key") == DEFAULT_DEVICE_KEY and d.get("reserved")
            for d in data.devices
        )
        if not has_default:
            return
        configured = {e.unique_id for e in self.hass.config_entries.async_entries(DOMAIN)}
        if DEFAULT_DEVICE_KEY in configured:
            return
        flows = self.hass.config_entries.flow.async_progress_by_handler(
            DOMAIN, include_uninitialized=True
        )
        if any(f["context"].get("unique_id") == DEFAULT_DEVICE_KEY for f in flows):
            return
        self.hass.async_create_task(
            self.hass.config_entries.flow.async_init(
                DOMAIN,
                context={"source": SOURCE_INTEGRATION_DISCOVERY},
                data={"key": DEFAULT_DEVICE_KEY, "code": None, "model": None},
            ),
            eager_start=False,
        )

    def _async_sync_discovery(self, data: ByonkData) -> None:
        pending_macs = {p["mac"] for p in data.pending}
        configured = {e.unique_id for e in self.hass.config_entries.async_entries(DOMAIN) if e.unique_id != DOMAIN}
        flows = self.hass.config_entries.flow.async_progress_by_handler(
            DOMAIN, include_uninitialized=True
        )
        discovery_flows = [
            f for f in flows
            if f["context"].get("source") == SOURCE_INTEGRATION_DISCOVERY
        ]
        in_progress = {f["context"].get("unique_id") for f in discovery_flows}

        for p in data.pending:
            mac = p["mac"]
            if mac in configured or mac in in_progress:
                continue
            # eager_start=False ensures the flow task runs on the next event-loop
            # iteration, after entry.runtime_data has been set by async_setup_entry.
            self.hass.async_create_task(
                self.hass.config_entries.flow.async_init(
                    DOMAIN,
                    context={"source": SOURCE_INTEGRATION_DISCOVERY},
                    data={
                        "key": mac,
                        "code": p.get("registration_code"),
                        "model": p.get("model"),
                    },
                ),
                eager_start=False,
            )

        for f in discovery_flows:
            uid = f["context"].get("unique_id")
            if uid and uid not in pending_macs:
                self.hass.config_entries.flow.async_abort(f["flow_id"])
