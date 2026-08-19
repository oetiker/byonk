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
)
from homeassistant.core import callback
from homeassistant.helpers import selector
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.hassio import is_hassio
from homeassistant.helpers.service_info.hassio import HassioServiceInfo

from .addon import (
    async_ensure_addon_installed,
    async_get_base_url,
    async_provision_token,
    async_read_token,
)
from .api import ByonkApiError, ByonkAuthError, ByonkClient
from .const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DEFAULT_DEVICE_KEY,
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

    @callback
    def _hub_entry(self):
        for entry in self._async_current_entries(include_ignore=False):
            if entry.unique_id == DOMAIN:
                return entry
        return None

    async def _async_create_hub_entry(self) -> ConfigFlowResult:
        """Install and start the app, provision the token, create the hub entry.

        Shared by the manual route (`user`) and the Supervisor discovery route
        (`hassio`), which differ only in how they are triggered.
        """
        try:
            slug = await async_ensure_addon_installed(self.hass)
            token = await async_read_token(self.hass, slug)
            if not token:
                token = await async_provision_token(self.hass, slug)
            base_url = await async_get_base_url(self.hass, slug)
        except AddonError:
            return self.async_abort(reason="addon_error")

        # Provisioning restarts the add-on; byonk's HTTP comes back up a moment
        # later. If it never answers, abort cleanly rather than raising.
        if not await _async_probe_ready(self.hass, base_url, token):
            return self.async_abort(reason="addon_unhealthy")

        await self.async_set_unique_id(DOMAIN)
        return self.async_create_entry(
            title="Byonk",
            data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: base_url},
        )

    async def async_step_user(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if self._hub_entry() is not None:
            return self.async_abort(reason="single_instance_allowed")
        if not is_hassio(self.hass):
            return self.async_abort(reason="not_hassio")
        return await self._async_create_hub_entry()

    async def async_step_hassio(
        self, discovery_info: HassioServiceInfo
    ) -> ConfigFlowResult:
        """The Byonk app told Supervisor it is running.

        The app writes this integration into the config dir and announces
        itself, so this is the normal way a user reaches setup: a Discovered
        card in Settings > Devices & Services.
        """
        if self._hub_entry() is not None:
            return self.async_abort(reason="single_instance_allowed")
        self.context["title_placeholders"] = {"name": discovery_info.name}
        return await self.async_step_hassio_confirm()

    async def async_step_hassio_confirm(
        self, user_input: dict[str, Any] | None = None
    ) -> ConfigFlowResult:
        if user_input is None:
            return self.async_show_form(
                step_id="hassio_confirm", data_schema=vol.Schema({})
            )
        return await self._async_create_hub_entry()

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
        if mac == DEFAULT_DEVICE_KEY:
            hub = self._hub_entry()
            if hub is None:
                return self.async_abort(reason="no_hub")
            return self.async_create_entry(
                title="Byonk Default",
                data={CONF_DEVICE_KEY: DEFAULT_DEVICE_KEY, CONF_HUB_ENTRY_ID: hub.entry_id},
            )
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

