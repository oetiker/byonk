"""Constants for the Byonk integration."""
from __future__ import annotations

from homeassistant.const import Platform

DOMAIN = "byonk"

BYONK_ADDON_REPO_URL = "https://github.com/oetiker/byonk"
ADDON_CONFIG_SLUG = "byonk"  # the add-on's config.yaml slug; full slug is "<repo_hash>_byonk"
ADDON_NAME = "Byonk"
DEFAULT_PORT = 3000

UPDATE_INTERVAL_SECONDS = 60

CONF_ADDON_SLUG = "addon_slug"
CONF_BASE_URL = "base_url"
CONF_DEVICE_KEY = "device_key"
CONF_HUB_ENTRY_ID = "hub_entry_id"

PLATFORMS: list[Platform] = [
    Platform.SENSOR,
    Platform.SELECT,
    Platform.SWITCH,
    Platform.TEXT,
]

BUILTIN_SCREEN_LABEL = "(built-in)"
