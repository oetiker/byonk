# HTTP API Reference

> Bring Your Own Server API for TRMNL e-ink devices

**Version:** 0.1.0

## Overview

Byonk provides a REST API for TRMNL device communication. The API handles device registration, content delivery, and logging.

| Endpoint | Description |
|----------|-------------|
| `GET /api/setup` | Device registration |
| `GET /api/display` | Get display content URL |
| `GET /api/image/{hash}.png` | Get rendered PNG by content hash |
| `POST /api/log` | Submit device logs |
| `GET /health` | Health check |

## Display

### GET /api/display

**Get display content for a device**

Returns JSON with an image_url that the device should fetch separately.
The firmware expects status=0 for success (not HTTP 200).

#### Parameters

| Name | In | Required | Description |
|------|-----|----------|-------------|
| `ID` | header | Yes | Device MAC address |
| `Access-Token` | header | Yes | API key from /api/setup |
| `Width` | header | No | Display width in pixels (default: 800) |
| `Height` | header | No | Display height in pixels (default: 480) |
| `Refresh-Rate` | header | No | Current refresh rate in seconds |
| `Battery-Voltage` | header | No | Battery voltage |
| `RSSI` | header | No | WiFi signal strength |
| `FW-Version` | header | No | Firmware version |
| `Model` | header | No | Device model ('og' or 'x') |
| `Board` | header | No | Board identifier (e.g., 'trmnl_og_4clr') |
| `Colors` | header | No | Display palette as comma-separated hex RGB (e.g., '#000000,#FFFFFF,#FF0000,#FFFF00'). Defaults to 4-grey palette if absent. |

#### Responses

**200**: Display content available

```json
{
  "filename": "string",
  "firmware_url": null,
  "image_url": null,
  "refresh_rate": 0,
  "reset_firmware": true,
  "special_function": null,
  "status": 0,
  "temperature_profile": null,
  "update_firmware": true
}
```

**400**: Missing required header

**404**: Device not found

### GET /api/image/{hash}.png

**Get rendered PNG image by content hash**

Returns the actual PNG image data rendered from SVG with dithering applied.
The content hash is provided in the `/api/display` response and ensures clients can detect when content has changed.

#### Parameters

| Name | In | Required | Description |
|------|-----|----------|-------------|
| `hash` | path | Yes | Content hash from `/api/display` response |

#### Responses

**200**: PNG image

**404**: Content not found (cache miss or invalid hash)

**500**: Rendering error

## Logging

### POST /api/log

**Submit device logs**

Devices send diagnostic logs when they encounter errors or issues.

#### Parameters

| Name | In | Required | Description |
|------|-----|----------|-------------|
| `ID` | header | Yes | Device MAC address |
| `Access-Token` | header | Yes | API key from /api/setup |

#### Request Body

```json
{
  "logs": [null]
}
```

#### Responses

**200**: Logs received successfully

```json
{
  "message": "string",
  "status": 0
}
```

## Device

### GET /api/setup

**Register a new device or retrieve existing registration**

The device sends its MAC address and receives an API key for future requests.

#### Parameters

| Name | In | Required | Description |
|------|-----|----------|-------------|
| `ID` | header | Yes | Device MAC address (e.g., 'AA:BB:CC:DD:EE:FF') |
| `FW-Version` | header | Yes | Firmware version (e.g., '1.7.1') |
| `Model` | header | Yes | Device model ('og' or 'x') |

#### Responses

**200**: Device registered successfully

```json
{
  "api_key": null,
  "friendly_id": null,
  "image_url": null,
  "message": null,
  "status": 0
}
```

**400**: Missing required header
