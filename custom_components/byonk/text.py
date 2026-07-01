"""Byonk text entities (string/color/url screen params)."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddEntitiesCallback

from .const import CONF_DEVICE_KEY
from .coordinator import ByonkConfigEntry
from .param_entities import ByonkParamText, setup_param_platform


async def async_setup_entry(
    hass: HomeAssistant, entry: ByonkConfigEntry, async_add_entities: AddEntitiesCallback
) -> None:
    if CONF_DEVICE_KEY in entry.data:
        setup_param_platform(
            entry, async_add_entities, {"string", "color", "url"}, ByonkParamText
        )
