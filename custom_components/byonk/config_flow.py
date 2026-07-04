"""Config flow for the Byonk integration."""
from __future__ import annotations

import asyncio
from collections.abc import Mapping
from typing import Any

import voluptuous as vol
from homeassistant.components.hassio import AddonError
from homeassistant.config_entries import (
    ConfigFlow,
    ConfigFlowResult,
    OptionsFlow,
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
from .const import (
    BUILTIN_SCREEN_LABEL,
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
)
from .param_form import build_params_schema, coerce_params

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

    def __init__(self) -> None:
        self._discovery: dict[str, Any] = {}
        self._key: str | None = None
        self._screen: str | None = None
        self._extra: dict[str, Any] = {}

    @staticmethod
    @callback
    def async_get_options_flow(config_entry) -> OptionsFlow:
        return ByonkOptionsFlow()

    @callback
    def _hub_entry(self):
        for entry in self._async_current_entries(include_ignore=False):
            if entry.unique_id == DOMAIN:
                return entry
        return None

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if any(e.unique_id == DOMAIN for e in self._async_current_entries(include_ignore=False)):
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

    async def async_step_integration_discovery(
        self, discovery_info: dict[str, Any]
    ) -> ConfigFlowResult:
        mac = discovery_info["key"]
        await self.async_set_unique_id(mac)
        self._abort_if_unique_id_configured()
        self._discovery = discovery_info
        self.context["title_placeholders"] = {
            "name": f"TRMNL {mac}",
            "code": discovery_info.get("code") or mac,
        }
        return await self.async_step_configure()

    async def async_step_configure(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        hub = self._hub_entry()
        if hub is None:
            return self.async_abort(reason="no_hub")
        data = hub.runtime_data.data
        if user_input is not None:
            self._key = self._discovery["key"]
            self._screen = user_input["screen"]
            self._extra = {
                k: user_input[k] for k in ("panel", "dither") if user_input.get(k)
            }
            return await self.async_step_dev_params()

        schema = vol.Schema(
            {
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
                        options=data.panel_names(),
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
            }
        )
        return self.async_show_form(
            step_id="configure",
            data_schema=schema,
            description_placeholders={
                "code": self._discovery.get("code") or self._discovery["key"]
            },
        )

    async def async_step_dev_params(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        hub = self._hub_entry()
        if hub is None:
            return self.async_abort(reason="no_hub")
        coordinator = hub.runtime_data
        fields = coordinator.data.screen_params(self._screen)
        if user_input is not None or not fields:
            params = coerce_params(fields, user_input or {})
            payload = {
                "key": self._key, "screen": self._screen, "params": params, **self._extra
            }
            try:
                await coordinator.client.async_add_device(payload)
            except ByonkApiError as err:
                if not fields:
                    return self.async_abort(reason="add_failed")
                return self.async_show_form(
                    step_id="dev_params",
                    data_schema=build_params_schema(fields, current=params),
                    errors={"base": "add_failed"},
                    description_placeholders={"error": str(err)},
                )
            return self.async_create_entry(
                title=f"TRMNL {self._key}",
                data={CONF_DEVICE_KEY: self._key, CONF_HUB_ENTRY_ID: hub.entry_id},
            )
        return self.async_show_form(
            step_id="dev_params", data_schema=build_params_schema(fields)
        )


class ByonkOptionsFlow(OptionsFlow):
    """Server-level settings that byonk owns (thin front over PATCH /settings)."""

    async def async_step_init(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        coordinator = self.config_entry.runtime_data
        if user_input is not None:
            screen = user_input["registration_screen"]
            await coordinator.client.async_update_settings(
                {
                    "registration_screen": "" if screen == BUILTIN_SCREEN_LABEL else screen,
                    "auth_mode": user_input["auth_mode"],
                    "package_refresh_interval": int(user_input["package_refresh_interval"]),
                }
            )
            await coordinator.async_request_refresh()
            return self.async_create_entry(title="", data={})

        data = coordinator.data
        current_screen = data.registration_screen() or BUILTIN_SCREEN_LABEL
        interval = data.config.get("package_refresh_interval", 0)
        schema = vol.Schema(
            {
                vol.Required("registration_screen", default=current_screen): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=[BUILTIN_SCREEN_LABEL, *data.screen_names()],
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Required("auth_mode", default=data.auth_mode() or "api_key"): selector.SelectSelector(
                    selector.SelectSelectorConfig(
                        options=["api_key", "ed25519"],
                        mode=selector.SelectSelectorMode.DROPDOWN,
                    )
                ),
                vol.Required("package_refresh_interval", default=interval): selector.NumberSelector(
                    selector.NumberSelectorConfig(
                        min=0, max=86400, step=1, unit_of_measurement="s",
                        mode=selector.NumberSelectorMode.BOX,
                    )
                ),
            }
        )
        return self.async_show_form(step_id="init", data_schema=schema)

