"""Config flow for the Byonk integration."""
from __future__ import annotations

from typing import Any

from homeassistant.components.hassio import AddonError
from homeassistant.config_entries import ConfigFlow, ConfigFlowResult
from homeassistant.helpers.aiohttp_client import async_get_clientsession
from homeassistant.helpers.hassio import is_hassio

from .addon import (
    async_ensure_addon_installed,
    async_get_base_url,
    async_provision_token,
    async_read_token,
)
from .api import ByonkClient
from .const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN


class ByonkConfigFlow(ConfigFlow, domain=DOMAIN):
    """Zero-touch, Supervised-only setup."""

    VERSION = 1

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
            client = ByonkClient(
                async_get_clientsession(self.hass), base_url, token
            )
            await client.async_get_config()  # auth probe
        except AddonError:
            return self.async_abort(reason="addon_error")

        await self.async_set_unique_id(DOMAIN)
        return self.async_create_entry(
            title="Byonk",
            data={CONF_ADDON_SLUG: slug, CONF_BASE_URL: base_url},
        )
