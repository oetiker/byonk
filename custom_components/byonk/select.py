"""Byonk select entities."""
from __future__ import annotations

from homeassistant.components.select import SelectEntity
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import BUILTIN_SCREEN_LABEL, CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry, ByonkCoordinator
from .entity import ByonkDeviceEntity, ByonkHubEntity
from .param_form import default_params


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    coordinator = entry.runtime_data
    if CONF_DEVICE_KEY in entry.data:
        key = entry.data[CONF_DEVICE_KEY]
        async_add_entities(
            [
                ByonkScreenSelect(coordinator, key),
                ByonkDitherSelect(coordinator, key),
                ByonkPanelSelect(coordinator, key),
            ]
        )
        return
    async_add_entities(
        [ByonkNewDeviceScreenSelect(coordinator), ByonkAuthModeSelect(coordinator)]
    )


class _ByonkSelect(ByonkDeviceEntity, SelectEntity):
    _field: str

    def __init__(self, coordinator: ByonkCoordinator, key: str) -> None:
        super().__init__(coordinator, key)
        self._attr_unique_id = f"{key}_{self._field}"
        self._attr_translation_key = self._field

    def _base_options(self) -> list[str]:
        """Options offered by byonk for this field."""
        raise NotImplementedError

    @property
    def options(self) -> list[str]:
        # HA blanks a select whose current_option is not among options (state
        # becomes "unknown"). byonk can legitimately hold a value that is not a
        # current option (e.g. a panel that is not in the running config, or a
        # value set out-of-band), so surface it alongside the real choices.
        opts = self._base_options()
        current = self.current_option
        if current and current not in opts:
            return [*opts, current]
        return opts

    @property
    def current_option(self) -> str | None:
        device = self.device
        return device.get(self._field) if device else None

    async def _write(self, payload: dict) -> None:
        await self.coordinator.client.async_update_device(self._key, payload)
        await self.coordinator.async_request_refresh()


class ByonkScreenSelect(_ByonkSelect):
    _field = "screen"

    def _base_options(self) -> list[str]:
        return self.coordinator.data.screen_names()

    async def async_select_option(self, option: str) -> None:
        params = default_params(self.coordinator.data.screen_params(option))
        await self._write({"screen": option, "params": params})


class ByonkDitherSelect(_ByonkSelect):
    _field = "dither"

    def _base_options(self) -> list[str]:
        return self.coordinator.data.dither

    async def async_select_option(self, option: str) -> None:
        await self._write({"dither": option})


class ByonkPanelSelect(_ByonkSelect):
    _field = "panel"

    def _base_options(self) -> list[str]:
        return self.coordinator.data.panel_names()

    async def async_select_option(self, option: str) -> None:
        await self._write({"panel": option})


class ByonkNewDeviceScreenSelect(ByonkHubEntity, SelectEntity):
    _attr_translation_key = "new_device_screen"
    _attr_entity_category = EntityCategory.CONFIG

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_new_device_screen"

    @property
    def options(self) -> list[str]:
        return [BUILTIN_SCREEN_LABEL, *self.coordinator.data.screen_names()]

    @property
    def current_option(self) -> str | None:
        screen = self.coordinator.data.registration_screen()
        return screen or BUILTIN_SCREEN_LABEL

    async def async_select_option(self, option: str) -> None:
        value = "" if option == BUILTIN_SCREEN_LABEL else option
        await self.coordinator.client.async_update_settings(
            {"registration_screen": value}
        )
        await self.coordinator.async_request_refresh()


class ByonkAuthModeSelect(ByonkHubEntity, SelectEntity):
    _attr_translation_key = "auth_mode"
    _attr_entity_category = EntityCategory.CONFIG
    _attr_options = ["api_key", "ed25519"]

    def __init__(self, coordinator) -> None:
        super().__init__(coordinator)
        self._attr_unique_id = f"{coordinator.entry.entry_id}_auth_mode"

    @property
    def current_option(self) -> str | None:
        return self.coordinator.data.auth_mode()

    async def async_select_option(self, option: str) -> None:
        await self.coordinator.client.async_update_settings({"auth_mode": option})
        await self.coordinator.async_request_refresh()
