# Byonk v0.8.0 Released - Template Inheritance, Includes, and Built-in Components

Just released v0.8.0 of [Byonk](https://github.com/oetiker/byonk), the self-hosted content server for TRMNL e-ink devices.

## What's Byonk?

Byonk (Bring Your Own Ink) lets you create custom screens for your TRMNL device using Lua scripts for data fetching and SVG templates for rendering. Fetch data from any API, format it how you want, and display it on your e-ink screen.

## What's New in 0.8.0

**Template Inheritance** - Create reusable base layouts with `{% extends "layouts/base.svg" %}` and override specific blocks. No more copy-pasting the same header/footer into every screen.

**Template Includes** - Embed reusable components with `{% include "components/header.svg" %}`. Build a library of components and mix-and-match.

**Built-in Layout & Components** - Ships with ready-to-use `layouts/base.svg`, `components/header.svg`, `components/footer.svg`, and `components/status_bar.svg` so you can get started quickly.

**HTTP Response Caching** - New `cache_ttl` option for `http_request`/`http_get` to cache API responses locally (LRU cache, max 100 entries). Great for APIs with rate limits.

**URL Encode/Decode** - New `url_encode()` and `url_decode()` Lua functions.

**Bug Fixes:**
- Fixed memory leak from unbounded cache growth (now uses LRU eviction)
- Fixed potential deadlocks under load
- Fixed trailing slash compatibility for TRMNL firmware 1.6.9+

## Quick Start

```bash
docker run --pull always -d -p 3000:3000 ghcr.io/oetiker/byonk:latest
```

Point your TRMNL to `http://your-server:3000` and you're good to go.

**Links:**
- GitHub: https://github.com/oetiker/byonk
- Documentation: https://oetiker.github.io/byonk/
- Release: https://github.com/oetiker/byonk/releases/tag/v0.8.0
