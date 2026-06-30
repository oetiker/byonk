"""Config flow for the Byonk integration."""
from __future__ import annotations

import asyncio
from collections.abc import Mapping
from typing import Any

import voluptuous as vol
from homeassistant.components.hassio import AddonError
from homeassistant.config_entries import (
    ConfigEntry,
    ConfigFlow,
    ConfigFlowResult,
    ConfigSubentryFlow,
    SubentryFlowResult,
)
from homeassistant.core import callback
from homeassistant.helpers import selector
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.hassio import is_hassio

from .addon import (
    async_ensure_addon_installed,
    async_get_base_url,
    async_provision_token,
    async_read_token,
)
from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN
from .param_form import build_params_schema

# Provisioning restarts the add-on; byonk's HTTP needs a moment to come back up
# and load the new token. Probe the admin API until it answers (or give up).
PROBE_ATTEMPTS = 15
PROBE_DELAY = 2  # seconds between attempts (~30s total)


async def _async_probe_ready(hass, base_url, token) -> bool:
    """Probe the admin API until it authenticates, tolerating add-on restart latency."""
    client = ByonkClient(async_get_clientsession(hass), base_url, token)
    for attempt in range(PROBE_ATTEMPTS):
        try:
            await client.async_get_config()
            return True
        except ByonkApiError:
            if attempt < PROBE_ATTEMPTS - 1:
                await asyncio.sleep(PROBE_DELAY)
    return False


async def _token_authenticates(hass, base_url, token) -> bool:
    """True if the token authenticates (or we cannot tell); False only on a definitive auth failure."""
    client = ByonkClient(async_get_clientsession(hass), base_url, token)
    try:
        await client.async_get_config()
    except ByonkAuthError:
        return False
    except ByonkApiError:
        return True  # transient/connection: don't reprovision
    return True


class ByonkConfigFlow(ConfigFlow, domain=DOMAIN):
    """Zero-touch, Supervised-only setup."""

    VERSION = 1

    @classmethod
    @callback
    def async_get_supported_subentry_types(
        cls, config_entry: ConfigEntry
    ) -> dict[str, type[ConfigSubentryFlow]]:
        return {"device": ByonkDeviceSubentryFlow}

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if self._async_current_entries():
            return self.async_abort(reason="single_instance_allowed")
        if not is_hassio(self.hass):
            return self.async_abort(reason="not_hassio")

        try:
            slug = await async_ensure_addon_installed(self.hass)
            token = await async_read_token(self.hass, slug)
            if not token:
                token = await async_provision_token(self.hass, slug)
            base_url = await async_get_base_url(self.hass, slug)
        except AddonError:
            return self.async_abort(reason="addon_error")

        # Provisioning restarts the add-on; retry the probe while byonk's HTTP
        # comes back up. If it never answers/authenticates, abort cleanly
        # (e.g. an image too old to expose the admin API) instead of raising 500.
        if not await _async_probe_ready(self.hass, base_url, token):
            return self.async_abort(reason="addon_unhealthy")

        await self.async_set_unique_id(DOMAIN)
        return self.async_create_entry(
            title="Byonk",
            data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: base_url},
        )

    async def async_step_reauth(
        self, entry_data: Mapping[str, Any]
    ) -> ConfigFlowResult:
        return await self.async_step_reauth_confirm()

    async def async_step_reauth_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        entry = self._get_reauth_entry()
        slug = entry.data[CONF_ADDON_SLUG]
        token = await async_read_token(self.hass, slug)
        if not token or not await _token_authenticates(
            self.hass, entry.data[CONF_BASE_URL], token
        ):
            await async_provision_token(self.hass, slug)
        return self.async_update_reload_and_abort(entry, data=entry.data)


class ByonkDeviceSubentryFlow(ConfigSubentryFlow):
    """Add or edit a device->screen mapping."""

    def __init__(self) -> None:
        self._key: str | None = None
        self._screen: str | None = None
        self._extra: dict[str, Any] = {}

    @property
    def _coordinator(self):
        return self._get_entry().runtime_data

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        data = self._coordinator.data
        if user_input is not None:
            self._key = user_input["key"]
            self._screen = user_input["screen"]
            self._extra = {
                k: user_input[k] for k in ("panel", "dither") if user_input.get(k)
            }
            return await self.async_step_params()

        pending_opts = [
            selector.SelectOptionDict(
                value=p["mac"],
                label=f'{p.get("registration_code") or p["mac"]} · {p.get("model","?")}',
            )
            for p in data.pending
        ]
        schema = vol.Schema(
            {
                vol.Required("key"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=pending_opts, custom_value=True,
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Required("screen"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.screen_names(),
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Optional("dither"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.dither, mode=selector.SelectSelectorMode.DROPDOWN
                    )
                ),
                vol.Optional("panel"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=data.panel_names(), mode=selector.SelectSelectorMode.DROPDOWN
                    )
                ),
            }
        )
        return self.async_show_form(step_id="user", data_schema=schema)

    async def async_step_params(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        fields = self._coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = user_input or {}
            payload = {"key": self._key, "screen": self._screen, "params": params, **self._extra}
            await self._coordinator.client.async_add_device(payload)
            await self._coordinator.async_request_refresh()
            return self.async_create_entry(
                title=self._key, data={"key": self._key}, unique_id=self._key
            )
        return self.async_show_form(
            step_id="params", data_schema=build_params_schema(fields)
        )

    async def async_step_reconfigure(
        self, user_input: dict[str, Any] | None = None
    ) -> SubentryFlowResult:
        sub = self._get_reconfigure_subentry()
        self._key = sub.data["key"]
        device = next(
            (d for d in self._coordinator.data.devices if d["key"] == self._key), {}
        )
        self._screen = device.get("screen")
        fields = self._coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = user_input or {}
            await self._coordinator.client.async_update_device(
                self._key, {"screen": self._screen, "params": params}
            )
            await self._coordinator.async_request_refresh()
            return self.async_update_and_abort(
                self._get_entry(), sub, data={"key": self._key}
            )
        return self.async_show_form(
            step_id="reconfigure",
            data_schema=build_params_schema(fields, current=device.get("params") or {}),
        )
