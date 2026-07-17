"""Build HA forms from byonk @params schemas."""
from __future__ import annotations

import voluptuous as vol
from homeassistant.helpers import selector


def coerce_params(param_fields: list[dict], values: dict) -> dict:
    """Cast submitted form values to their declared @params types.

    Home Assistant's NumberSelector always returns floats, but byonk requires a
    real integer for ``int`` fields. Coerce whole-number floats back to int;
    leave everything else (including non-whole floats, so byonk can report the
    validation error) untouched.
    """
    types = {f["name"]: f.get("type", "string") for f in param_fields}
    out: dict = {}
    for name, value in values.items():
        if (
            types.get(name) == "int"
            and isinstance(value, float)
            and value.is_integer()
        ):
            out[name] = int(value)
        else:
            out[name] = value
    return out


def default_params(param_fields: list[dict]) -> dict:
    """Return {name: default} for fields that declare a default."""
    return {
        f["name"]: f["default"]
        for f in param_fields
        if "default" in f and f["default"] is not None
    }


def _selector_for(field: dict):
    """Return a HA selector for the given field descriptor."""
    ftype = field.get("type", "string")
    if ftype in ("int", "float"):
        cfg = selector.NumberSelectorConfig(
            mode=selector.NumberSelectorMode.BOX,
            step=1 if ftype == "int" else "any",
        )
        if field.get("min") is not None:
            cfg["min"] = field["min"]
        if field.get("max") is not None:
            cfg["max"] = field["max"]
        if field.get("unit"):
            cfg["unit_of_measurement"] = field["unit"]
        return selector.NumberSelector(cfg)
    if ftype == "bool":
        return selector.BooleanSelector()
    if ftype == "enum":
        opts = []
        for o in field.get("options", []):
            if isinstance(o, dict):
                opts.append(
                    selector.SelectOptionDict(
                        value=str(o["value"]), label=o.get("label", str(o["value"]))
                    )
                )
            else:
                opts.append(selector.SelectOptionDict(value=str(o), label=str(o)))
        return selector.SelectSelector(
            selector.SelectSelectorConfig(
                options=opts, mode=selector.SelectSelectorMode.DROPDOWN
            )
        )
    if ftype == "color":
        return selector.TextSelector(
            selector.TextSelectorConfig(type=selector.TextSelectorType.COLOR)
        )
    if ftype == "url":
        return selector.TextSelector(
            selector.TextSelectorConfig(type=selector.TextSelectorType.URL)
        )
    # string (default)
    text_type = (
        selector.TextSelectorType.PASSWORD
        if field.get("sensitive")
        else selector.TextSelectorType.TEXT
    )
    return selector.TextSelector(
        selector.TextSelectorConfig(type=text_type, multiline=bool(field.get("multiline")))
    )


def build_params_schema(
    param_fields: list[dict], current: dict | None = None
) -> vol.Schema:
    """Build a voluptuous Schema from @params field descriptors."""
    current = current or {}
    schema: dict = {}
    for field in param_fields:
        if field.get("hidden"):
            continue
        name = field["name"]
        marker_cls = vol.Required if field.get("required") else vol.Optional
        description = None
        if name in current:
            description = {"suggested_value": current[name]}
        elif "default" in field and field["default"] is not None:
            description = {"suggested_value": field["default"]}
        marker = marker_cls(name, description=description)
        schema[marker] = _selector_for(field)
    return vol.Schema(schema)
