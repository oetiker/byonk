"""Shared fixtures for Byonk integration tests."""
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch

import pytest
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import (
    CONF_ADDON_SLUG,
    CONF_BASE_URL,
    CONF_DEVICE_KEY,
    CONF_HUB_ENTRY_ID,
    DOMAIN,
)

pytest_plugins = ["pytest_homeassistant_custom_component"]

DEFAULT_SCREENS = {
    "packages": [{"handle": "byonk-builtin", "name": "byonk-builtin",
                  "description": "Built-in screens", "author": "Byonk", "license": "MIT",
                  "screens": [{"ref": "byonk-builtin/useful/swiss-departure-board",
                                "title": "Swiss Departure Board", "description": "",
                                "params": [], "byonk": "0.15", "compat_warning": None}]}],
    "panels": [{"name": "trmnl_og"}],
    "dither_algorithms": ["atkinson"],
}


@pytest.fixture(autouse=True)
def auto_enable_custom_integrations(enable_custom_integrations):
    """Enable loading custom integrations in all tests."""
    yield


def make_hub_entry(hass):
    """Add an unloaded hub config entry to hass."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        title="Byonk",
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )
    entry.add_to_hass(hass)
    return entry


def make_device_entry(hass, hub, key):
    """Add an unloaded device config entry to hass."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=key,
        title=f"TRMNL {key}",
        data={CONF_DEVICE_KEY: key, CONF_HUB_ENTRY_ID: hub.entry_id},
    )
    entry.add_to_hass(hass)
    return entry


@pytest.fixture
def byonk():
    """Patch ByonkClient + token reader; expose mutable state to each test."""
    state = SimpleNamespace(
        devices=[],
        pending=[],
        screens=DEFAULT_SCREENS,
        config={},
        add_device=AsyncMock(return_value={"key": "x", "screen": "byonk-builtin/useful/swiss-departure-board"}),
        update_device=AsyncMock(),
        delete_device=AsyncMock(),
        update_settings=AsyncMock(),
    )
    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch.multiple(
            "custom_components.byonk.coordinator.ByonkClient",
            async_get_devices=AsyncMock(side_effect=lambda *a, **k: list(state.devices)),
            async_get_pending=AsyncMock(side_effect=lambda *a, **k: list(state.pending)),
            async_get_screens=AsyncMock(side_effect=lambda *a, **k: state.screens),
            async_get_config=AsyncMock(side_effect=lambda *a, **k: state.config),
            async_add_device=state.add_device,
            async_update_device=state.update_device,
            async_delete_device=state.delete_device,
            async_update_settings=state.update_settings,
        ),
    ):
        yield state
