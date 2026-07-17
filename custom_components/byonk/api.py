"""Async client for the byonk admin API (Phase 1)."""
from __future__ import annotations

from typing import Any

import aiohttp


class ByonkApiError(Exception):
    """Base error for admin API calls."""


class ByonkConnectionError(ByonkApiError):
    """Network/transport failure."""


class ByonkAuthError(ByonkApiError):
    """Admin API dormant (404) or wrong token (401)."""


class ByonkValidationError(ByonkApiError):
    """byonk rejected a write (400)."""

    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message


class ByonkReadOnlyError(ByonkApiError):
    """Config is embedded/read-only, or a delete is blocked by a reference (409)."""

    def __init__(self, message: str = "") -> None:
        super().__init__(message)
        self.message = message


class ByonkClient:
    """Thin wrapper over /api/admin/* using a shared aiohttp session."""

    def __init__(
        self, session: aiohttp.ClientSession, base_url: str, token: str
    ) -> None:
        self._session = session
        self._base = base_url.rstrip("/")
        self._token = token

    async def _request(
        self, method: str, path: str, json: dict | None = None
    ) -> Any:
        url = f"{self._base}{path}"
        headers = {"Authorization": f"Bearer {self._token}"}
        try:
            async with self._session.request(
                method, url, json=json, headers=headers
            ) as resp:
                if resp.status in (401, 404):
                    raise ByonkAuthError(f"{method} {path} -> {resp.status}")
                if resp.status == 409:
                    body = await _safe_json(resp)
                    raise ByonkReadOnlyError(
                        body.get("error") or body.get("message") or ""
                    )
                if resp.status == 400:
                    body = await _safe_json(resp)
                    raise ByonkValidationError(
                        body.get("error") or body.get("message") or "validation error"
                    )
                resp.raise_for_status()
                if resp.status == 204:
                    return None
                return await resp.json()
        except aiohttp.ClientError as err:
            raise ByonkConnectionError(str(err)) from err

    async def async_get_devices(self) -> list[dict]:
        return await self._request("GET", "/api/admin/devices")

    async def async_get_pending(self) -> list[dict]:
        return await self._request("GET", "/api/admin/pending")

    async def async_get_screens(self) -> dict:
        return await self._request("GET", "/api/admin/screens")

    async def async_get_config(self) -> dict:
        return await self._request("GET", "/api/admin/config")

    async def async_add_device(self, payload: dict) -> dict:
        return await self._request("POST", "/api/admin/devices", json=payload)

    async def async_update_device(self, key: str, payload: dict) -> dict:
        return await self._request("PATCH", f"/api/admin/devices/{key}", json=payload)

    async def async_delete_device(self, key: str) -> dict:
        return await self._request("DELETE", f"/api/admin/devices/{key}")

    async def async_update_settings(self, payload: dict) -> dict:
        return await self._request("PATCH", "/api/admin/settings", json=payload)

    async def async_get_packages(self) -> list[dict]:
        return await self._request("GET", "/api/admin/packages")

    async def async_add_package(self, payload: dict) -> dict:
        return await self._request("POST", "/api/admin/packages", json=payload)

    async def async_update_package(self, handle: str, payload: dict) -> dict:
        return await self._request("PATCH", f"/api/admin/packages/{handle}", json=payload)

    async def async_delete_package(self, handle: str) -> dict:
        return await self._request("DELETE", f"/api/admin/packages/{handle}")

    async def async_update_packages(self) -> dict:
        return await self._request("POST", "/api/admin/packages/update")


async def _safe_json(resp: aiohttp.ClientResponse) -> dict:
    try:
        return await resp.json()
    except (aiohttp.ContentTypeError, ValueError):
        return {}
