import aiohttp
import pytest
from aioresponses import aioresponses

from custom_components.byonk.api import (
    ByonkAuthError,
    ByonkClient,
    ByonkReadOnlyError,
    ByonkValidationError,
)

BASE = "http://addon:3000"


@pytest.fixture
def mock_aioresponse():
    with aioresponses() as m:
        yield m


async def test_get_devices_sends_bearer(mock_aioresponse):
    mock_aioresponse.get(
        f"{BASE}/api/admin/devices",
        payload=[{"key": "AA:BB", "screen": "transit"}],
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        result = await client.async_get_devices()
    assert result[0]["key"] == "AA:BB"
    req = next(iter(mock_aioresponse.requests.values()))[0]
    assert req.kwargs["headers"]["Authorization"] == "Bearer secret"


async def test_404_raises_auth_error(mock_aioresponse):
    mock_aioresponse.get(f"{BASE}/api/admin/devices", status=404)
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "")
        with pytest.raises(ByonkAuthError):
            await client.async_get_devices()


async def test_400_raises_validation_with_message(mock_aioresponse):
    mock_aioresponse.post(
        f"{BASE}/api/admin/devices", status=400, payload={"error": "unknown screen"}
    )
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        with pytest.raises(ByonkValidationError) as exc:
            await client.async_add_device({"key": "AA:BB", "screen": "nope"})
    assert "unknown screen" in exc.value.message


async def test_409_raises_readonly(mock_aioresponse):
    mock_aioresponse.patch(f"{BASE}/api/admin/settings", status=409, payload={})
    async with aiohttp.ClientSession() as session:
        client = ByonkClient(session, BASE, "secret")
        with pytest.raises(ByonkReadOnlyError):
            await client.async_update_settings({"registration_enabled": True})
