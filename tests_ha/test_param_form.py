import voluptuous as vol
from homeassistant.helpers import selector

from custom_components.byonk.param_form import build_params_schema

FIELDS = [
    {"name": "station", "type": "string", "required": True, "label": "Stop"},
    {"name": "limit", "type": "int", "default": 8, "min": 1, "max": 30},
    {"name": "theme", "type": "enum", "options": ["light", "dark"]},
    {"name": "enabled", "type": "bool", "default": True},
]


def test_builds_selectors_per_type():
    schema = build_params_schema(FIELDS)
    markers = {str(m): m for m in schema.schema}
    assert "station" in markers
    # required field uses vol.Required
    assert any(isinstance(m, vol.Required) and m.schema == "station" for m in schema.schema)
    sel = schema.schema[next(m for m in schema.schema if m.schema == "limit")]
    assert isinstance(sel, selector.NumberSelector)
    enum_sel = schema.schema[next(m for m in schema.schema if m.schema == "theme")]
    assert isinstance(enum_sel, selector.SelectSelector)
    bool_sel = schema.schema[next(m for m in schema.schema if m.schema == "enabled")]
    assert isinstance(bool_sel, selector.BooleanSelector)


def test_optional_fields_are_optional():
    schema = build_params_schema(FIELDS)
    assert any(isinstance(m, vol.Optional) and m.schema == "limit" for m in schema.schema)
