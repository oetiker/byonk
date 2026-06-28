# Security Policy

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues, discussions, or pull requests.**

Report privately using one of:

1. **GitHub private vulnerability reporting (preferred)** — go to the repository's
   **Security** tab → **Report a vulnerability**
   (<https://github.com/oetiker/byonk/security/advisories/new>). This keeps the report
   confidential and lets us collaborate on a fix and advisory.
2. **Email** — <oetiker@gmail.com>, with `byonk security` in the subject line.

Please include:

- A description of the vulnerability and its impact.
- Steps to reproduce (proof-of-concept, affected endpoint/config, request samples).
- The byonk version (`byonk --version`) and how it is deployed (binary, Docker image, etc.).
- Any suggested remediation, if you have one.

### What to expect

- **Acknowledgement** within a few days.
- An assessment and, for confirmed issues, a fix in the latest release.
- Credit in the release notes / advisory if you would like it (let us know how to attribute you).

This is a volunteer-maintained project; please allow reasonable time for a fix before any
public disclosure. We aim to coordinate disclosure with you.

## Supported Versions

Byonk is pre-1.0; only the **latest release** receives security fixes. Please upgrade to the
current release before reporting, and expect fixes to land in a new release rather than as a
backport.

| Version | Supported |
|---------|-----------|
| Latest release | ✅ |
| Older releases | ❌ |

## Scope & deployment notes

Byonk is a **self-hosted** content server for TRMNL e-ink devices. A few things worth knowing
when assessing or deploying it securely:

- **Admin/management API** (`/api/admin/*`) is **disabled by default** and returns `404` unless
  an admin token is configured via the `BYONK_ADMIN_TOKEN` environment variable or `admin.token`
  in `config.yaml`. When enabled it requires an `Authorization: Bearer <token>` header. Use a
  strong, random token and prefer the environment variable over the config file.
- **Device authentication** uses per-device API keys (`api_key` mode) or Ed25519 signatures
  (`ed25519` mode), advertised via `/api/setup`.
- **Lua screens** are server-side scripts you provide; treat screen scripts and their data
  sources as trusted code running on your server.
- Byonk is typically deployed on a **trusted LAN** alongside the e-ink devices. If you expose it
  beyond your network, place it behind TLS and restrict access; do not expose the admin API to
  untrusted networks.

In-scope reports include (non-exhaustively): authentication/authorization bypass, admin-API
token leakage, SSRF via screen data fetching, path traversal in asset/config handling, and
remote code execution. Issues that require already-trusted access (e.g. the contents of screen
scripts you author yourself) are generally out of scope.
