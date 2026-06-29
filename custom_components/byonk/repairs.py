"""Repairs issues for pending byonk devices."""
from __future__ import annotations

from homeassistant.core import HomeAssistant
from homeassistant.helpers import issue_registry as ir

from .const import DOMAIN, ISSUE_PENDING_PREFIX


def async_sync_pending_issues(
    hass: HomeAssistant, pending: list[dict]
) -> None:
    reg = ir.async_get(hass)
    wanted: dict[str, dict] = {}
    for p in pending:
        code = p.get("registration_code") or p.get("mac")
        if not code:
            continue
        wanted[f"{ISSUE_PENDING_PREFIX}{code}"] = p

    for issue_id, p in wanted.items():
        ir.async_create_issue(
            hass,
            DOMAIN,
            issue_id,
            is_fixable=False,
            severity=ir.IssueSeverity.WARNING,
            translation_key="device_pending",
            translation_placeholders={
                "code": p.get("registration_code") or p.get("mac"),
                "model": p.get("model") or "TRMNL",
            },
        )

    for issue in list(reg.issues.values()):
        if (
            issue.domain == DOMAIN
            and issue.issue_id.startswith(ISSUE_PENDING_PREFIX)
            and issue.issue_id not in wanted
        ):
            ir.async_delete_issue(hass, DOMAIN, issue.issue_id)
