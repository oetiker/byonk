"""Test that a subentry added at runtime triggers an entry reload and creates entities.

FIX 1: async_setup_entry must register an update listener so that when reconcile
adds a new subentry (device appeared in /api/admin/devices), the reload fires and
the platform setup runs again, creating sensor entities for the new device.
"""
from unittest.mock import AsyncMock, patch

from homeassistant.config_entries import ConfigEntryState
from pytest_homeassistant_custom_component.common import MockConfigEntry

from custom_components.byonk.const import CONF_ADDON_SLUG, CONF_BASE_URL, DOMAIN

DEV = {
    "key": "AA:BB",
    "registered": True,
    "model": "og",
    "battery_voltage": 4.1,
    "rssi": -58,
    "last_seen": "2026-06-29T10:00:00+00:00",
    "firmware_version": "1.7.1",
    "screen": "transit",
    "dither": "atkinson",
    "panel": None,
}
SCREENS = {
    "screens": [{"name": "transit", "params": [], "schema_error": None}],
    "panels": [],
    "dither_algorithms": ["atkinson"],
}


async def test_runtime_device_creates_entities_after_reload(hass):
    """Device appears at runtime → reconcile adds subentry → reload → sensor entities exist."""
    entry = MockConfigEntry(
        domain=DOMAIN,
        unique_id=DOMAIN,
        data={CONF_ADDON_SLUG: "abcd_byonk", CONF_BASE_URL: "http://addon:3000"},
    )
    entry.add_to_hass(hass)

    # Start: no registered devices, so no subentries / no device entities at cold start.
    get_devices_mock = AsyncMock(return_value=[])

    with (
        patch("custom_components.byonk.async_read_token", new=AsyncMock(return_value="tok")),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_devices", new=get_devices_mock),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_pending", new=AsyncMock(return_value=[])),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_screens", new=AsyncMock(return_value=SCREENS)),
        patch("custom_components.byonk.coordinator.ByonkClient.async_get_config", new=AsyncMock(return_value={})),
    ):
        assert await hass.config_entries.async_setup(entry.entry_id)
        await hass.async_block_till_done()

        assert entry.state is ConfigEntryState.LOADED
        # No device sensors at cold start
        device_sensors = [
            s for s in hass.states.async_all("sensor") if "aa_bb" in s.entity_id
        ]
        assert len(device_sensors) == 0, (
            f"Expected no device sensors at cold start, got: {[s.entity_id for s in device_sensors]}"
        )

        # Device appears at runtime — flip the mock return value.
        get_devices_mock.return_value = [DEV]

        # Refresh: reconcile will add a subentry for AA:BB, which fires the update
        # listener registered by FIX 1, which triggers a full entry reload.
        coordinator = entry.runtime_data
        await coordinator.async_refresh()
        # Let the update-listener task and the reload run to completion.
        await hass.async_block_till_done()

        # After reload, sensor platform ran again with the subentry present.
        # Expect at least one sensor entity for the device key AA:BB.
        all_sensors = hass.states.async_all("sensor")
        device_sensors = [s for s in all_sensors if "aa_bb" in s.entity_id]
        assert len(device_sensors) > 0, (
            f"Expected device sensors after runtime reload, "
            f"got: {[s.entity_id for s in all_sensors]}"
        )
