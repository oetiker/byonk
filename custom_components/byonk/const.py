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

PLATFORMS: list[Platform] = [Platform.SENSOR, Platform.SELECT, Platform.SWITCH]

# Repairs
ISSUE_PENDING_PREFIX = "device_pending_"
