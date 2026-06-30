import voluptuous as vol
from homeassistant.helpers import selector

from custom_components.byonk.param_form import build_params_schema, coerce_params

FIELDS = [
    {"name": "station", "type": "string", "required": True, "label": "Stop"},
    {"name": "limit", "type": "int", "default": 8, "min": 1, "max": 30},
    {"name": "theme", "type": "enum", "options": ["light", "dark"]},
    {"name": "enabled", "type": "bool", "default": True},
]


def test_coerce_int_params_from_number_selector_float():
    """HA's NumberSelector yields floats; int fields must be coerced to int so
    byonk (which requires a real integer) accepts them."""
    coerced = coerce_params(FIELDS, {"limit": 8.0, "station": "Olten", "enabled": True})
    assert coerced["limit"] == 8
    assert isinstance(coerced["limit"], int)
    # non-int fields pass through untouched
    assert coerced["station"] == "Olten"
    assert coerced["enabled"] is True


def test_coerce_leaves_non_integer_float_for_int_field():
    """A non-whole float for an int field is left as-is so byonk reports the
    validation error rather than us silently truncating."""
    coerced = coerce_params(FIELDS, {"limit": 8.5})
    assert coerced["limit"] == 8.5


def test_coerce_float_field_stays_float():
    fields = [{"name": "opacity", "type": "float"}]
    coerced = coerce_params(fields, {"opacity": 0.5})
    assert coerced["opacity"] == 0.5


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


def _get_selector(schema, name):
    """Return the selector for the field named ``name``."""
    return schema.schema[next(m for m in schema.schema if m.schema == name)]


def test_float_produces_number_selector():
    fields = [{"name": "opacity", "type": "float", "min": 0.0, "max": 1.0}]
    schema = build_params_schema(fields)
    sel = _get_selector(schema, "opacity")
    assert isinstance(sel, selector.NumberSelector)
    assert sel.config.get("step") == "any"


def test_color_produces_text_selector_color_type():
    fields = [{"name": "bg", "type": "color"}]
    schema = build_params_schema(fields)
    sel = _get_selector(schema, "bg")
    assert isinstance(sel, selector.TextSelector)
    assert sel.config["type"] == "color"


def test_url_produces_text_selector_url_type():
    fields = [{"name": "feed", "type": "url"}]
    schema = build_params_schema(fields)
    sel = _get_selector(schema, "feed")
    assert isinstance(sel, selector.TextSelector)
    assert sel.config["type"] == "url"


def test_sensitive_string_produces_password_selector():
    fields = [{"name": "secret", "type": "string", "sensitive": True}]
    schema = build_params_schema(fields)
    sel = _get_selector(schema, "secret")
    assert isinstance(sel, selector.TextSelector)
    assert sel.config["type"] == "password"


def test_multiline_string_produces_multiline_selector():
    fields = [{"name": "notes", "type": "string", "multiline": True}]
    schema = build_params_schema(fields)
    sel = _get_selector(schema, "notes")
    assert isinstance(sel, selector.TextSelector)
    assert sel.config.get("multiline") is True


def test_hidden_field_is_skipped():
    fields = [
        {"name": "visible", "type": "string"},
        {"name": "hidden_field", "type": "string", "hidden": True},
    ]
    schema = build_params_schema(fields)
    names = [m.schema for m in schema.schema]
    assert "visible" in names
    assert "hidden_field" not in names


def test_suggested_value_from_default():
    """Optional field with a default → description has suggested_value == default."""
    fields = [{"name": "limit", "type": "int", "default": 10}]
    schema = build_params_schema(fields)
    marker = next(m for m in schema.schema if m.schema == "limit")
    assert marker.description == {"suggested_value": 10}


def test_current_wins_over_default():
    """When current is passed, current value takes precedence over default."""
    fields = [{"name": "limit", "type": "int", "default": 10}]
    schema = build_params_schema(fields, current={"limit": 42})
    marker = next(m for m in schema.schema if m.schema == "limit")
    assert marker.description == {"suggested_value": 42}
