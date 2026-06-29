"""Build HA forms from byonk @params schemas."""
from __future__ import annotations


def default_params(param_fields: list[dict]) -> dict:
    """Return {name: default} for fields that declare a default."""
    return {
        f["name"]: f["default"]
        for f in param_fields
        if "default" in f and f["default"] is not None
    }
