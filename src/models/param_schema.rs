//! Per-screen `@params` schema: types, textual extraction from `.lua`, and parsing.
//!
//! The schema is declared inside a Lua block comment at the top of a screen script:
//! ```lua
//! --[[ @params
//! station:
//!   type: string
//!   required: true
//! ]]
//! ```
//! It is parsed as YAML — never executed.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    #[default]
    String,
    Int,
    Float,
    Bool,
    Enum,
    Color,
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ParamField {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<EnumOption>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub advanced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct ParamSchema {
    pub fields: Vec<ParamField>,
}

/// Extract the YAML text inside a `--[[ @params ... ]]` block. Returns `None`
/// if no `@params` marker is present.
pub fn extract_params_block(lua_source: &str) -> Option<String> {
    let marker = lua_source.find("@params")?;
    // Start after the rest of the marker line.
    let after_marker = &lua_source[marker + "@params".len()..];
    let body_start = after_marker.find('\n').map(|i| i + 1).unwrap_or(0);
    let body = &after_marker[body_start..];
    let end = body.find("]]")?;
    Some(body[..end].to_string())
}

/// Raw descriptor as written in YAML (without the `name`, which is the map key).
#[derive(Deserialize)]
struct RawField {
    #[serde(rename = "type")]
    param_type: ParamType,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    options: Option<serde_yaml::Value>,
    #[serde(default)]
    sensitive: bool,
    #[serde(default)]
    multiline: bool,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    advanced: bool,
}

fn parse_options(raw: serde_yaml::Value) -> Result<Vec<EnumOption>, String> {
    let seq = raw
        .as_sequence()
        .ok_or_else(|| "enum `options` must be a list".to_string())?;
    let mut out = Vec::new();
    for item in seq {
        if let Some(s) = item.as_str() {
            out.push(EnumOption {
                value: s.to_string(),
                label: s.to_string(),
            });
        } else if let Some(map) = item.as_mapping() {
            let value = map
                .get(serde_yaml::Value::from("value"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "enum option object needs a string `value`".to_string())?
                .to_string();
            let label = map
                .get(serde_yaml::Value::from("label"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.clone());
            out.push(EnumOption { value, label });
        } else {
            return Err("enum option must be a scalar or {value,label} map".to_string());
        }
    }
    Ok(out)
}

/// Parse a `@params` YAML body into a schema. Preserves field order.
pub fn parse_schema(yaml: &str) -> Result<ParamSchema, String> {
    // Empty body ⇒ empty schema (screen takes no params).
    if yaml.trim().is_empty() {
        return Ok(ParamSchema::default());
    }
    let mapping: serde_yaml::Mapping =
        serde_yaml::from_str(yaml).map_err(|e| format!("invalid @params YAML: {e}"))?;

    let mut fields = Vec::new();
    for (k, v) in mapping {
        let name = k
            .as_str()
            .ok_or_else(|| "param keys must be strings".to_string())?
            .to_string();
        let raw: RawField =
            serde_yaml::from_value(v).map_err(|e| format!("param `{name}`: {e}"))?;

        let options = match raw.options {
            Some(o) => parse_options(o)?,
            None => Vec::new(),
        };
        if raw.param_type == ParamType::Enum && options.is_empty() {
            return Err(format!("param `{name}`: enum requires non-empty `options`"));
        }

        fields.push(ParamField {
            name,
            param_type: raw.param_type,
            required: raw.required,
            default: raw.default,
            label: raw.label,
            description: raw.description,
            min: raw.min,
            max: raw.max,
            step: raw.step,
            unit: raw.unit,
            mode: raw.mode,
            options,
            sensitive: raw.sensitive,
            multiline: raw.multiline,
            hidden: raw.hidden,
            advanced: raw.advanced,
        });
    }
    Ok(ParamSchema { fields })
}

/// Extract + parse a screen's schema. `Ok(None)` when there is no `@params`
/// block; `Err` when a block is present but malformed.
pub fn schema_for_script(lua_source: &str) -> Result<Option<ParamSchema>, String> {
    match extract_params_block(lua_source) {
        None => Ok(None),
        Some(body) => parse_schema(&body).map(Some),
    }
}

/// Validate a params map against a schema. Returns all problems found (not just
/// the first). Params not described by the schema are allowed (ignored).
pub fn validate_params(
    schema: &ParamSchema,
    params: &HashMap<String, serde_yaml::Value>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    for field in &schema.fields {
        match params.get(&field.name) {
            None => {
                if field.required {
                    errors.push(format!("missing required param `{}`", field.name));
                }
            }
            Some(value) => {
                if let Err(e) = check_value(field, value) {
                    errors.push(e);
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_value(field: &ParamField, value: &serde_yaml::Value) -> Result<(), String> {
    let name = &field.name;
    match field.param_type {
        ParamType::String | ParamType::Color | ParamType::Url => {
            if !value.is_string() {
                return Err(format!("param `{name}` must be a string"));
            }
        }
        ParamType::Bool => {
            if !value.is_bool() {
                return Err(format!("param `{name}` must be a boolean"));
            }
        }
        ParamType::Int => {
            let n = value
                .as_i64()
                .ok_or_else(|| format!("param `{name}` must be an integer"))?;
            check_range(field, n as f64)?;
        }
        ParamType::Float => {
            let n = value
                .as_f64()
                .ok_or_else(|| format!("param `{name}` must be a number"))?;
            check_range(field, n)?;
        }
        ParamType::Enum => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("param `{name}` must be one of the enum values"))?;
            if !field.options.iter().any(|o| o.value == s) {
                return Err(format!(
                    "param `{name}` value `{s}` is not an allowed option"
                ));
            }
        }
    }
    Ok(())
}

fn check_range(field: &ParamField, n: f64) -> Result<(), String> {
    if let Some(min) = field.min {
        if n < min {
            return Err(format!("param `{}` must be >= {min}", field.name));
        }
    }
    if let Some(max) = field.max {
        if n > max {
            return Err(format!("param `{}` must be <= {max}", field.name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_block_present() {
        let lua = "--[[ @params\nstation:\n  type: string\n]]\nlocal x = 1\n";
        let block = extract_params_block(lua).unwrap();
        assert!(block.contains("station:"));
        assert!(block.contains("type: string"));
        assert!(!block.contains("local x"));
    }

    #[test]
    fn test_extract_block_absent() {
        assert!(extract_params_block("local x = 1\n").is_none());
    }

    #[test]
    fn test_parse_minimal_field() {
        let schema = parse_schema("station:\n  type: string\n  required: true\n").unwrap();
        assert_eq!(schema.fields.len(), 1);
        let f = &schema.fields[0];
        assert_eq!(f.name, "station");
        assert_eq!(f.param_type, ParamType::String);
        assert!(f.required);
    }

    #[test]
    fn test_parse_preserves_order() {
        let schema = parse_schema("b:\n  type: int\na:\n  type: string\n").unwrap();
        assert_eq!(schema.fields[0].name, "b");
        assert_eq!(schema.fields[1].name, "a");
    }

    #[test]
    fn test_parse_enum_options_objects_and_bare() {
        let obj = parse_schema(
            "k:\n  type: enum\n  options:\n    - {value: a, label: Apple}\n    - {value: b, label: Banana}\n",
        )
        .unwrap();
        assert_eq!(obj.fields[0].options.len(), 2);
        assert_eq!(obj.fields[0].options[0].label, "Apple");

        let bare = parse_schema("k:\n  type: enum\n  options: [a, b]\n").unwrap();
        assert_eq!(bare.fields[0].options[0].value, "a");
        assert_eq!(bare.fields[0].options[0].label, "a"); // label defaults to value
    }

    #[test]
    fn test_parse_enum_without_options_is_error() {
        assert!(parse_schema("k:\n  type: enum\n").is_err());
    }

    #[test]
    fn test_parse_unknown_type_is_error() {
        assert!(parse_schema("k:\n  type: banana\n").is_err());
    }

    #[test]
    fn test_schema_for_script_none_when_no_block() {
        assert!(schema_for_script("local x = 1\n").unwrap().is_none());
    }

    #[test]
    fn test_schema_for_script_err_when_malformed() {
        let lua = "--[[ @params\nk:\n  type: banana\n]]\n";
        assert!(schema_for_script(lua).is_err());
    }

    use std::collections::HashMap;

    fn params(pairs: &[(&str, serde_yaml::Value)]) -> HashMap<String, serde_yaml::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn test_validate_missing_required() {
        let schema = parse_schema("station:\n  type: string\n  required: true\n").unwrap();
        let errs = validate_params(&schema, &params(&[])).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("station")));
    }

    #[test]
    fn test_validate_type_mismatch() {
        let schema = parse_schema("limit:\n  type: int\n").unwrap();
        let errs = validate_params(&schema, &params(&[("limit", "abc".into())])).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("limit")));
    }

    #[test]
    fn test_validate_min_max() {
        let schema = parse_schema("limit:\n  type: int\n  min: 1\n  max: 30\n").unwrap();
        assert!(validate_params(&schema, &params(&[("limit", 50i64.into())])).is_err());
        assert!(validate_params(&schema, &params(&[("limit", 8i64.into())])).is_ok());
    }

    #[test]
    fn test_validate_enum_membership() {
        let schema = parse_schema("k:\n  type: enum\n  options: [a, b]\n").unwrap();
        assert!(validate_params(&schema, &params(&[("k", "c".into())])).is_err());
        assert!(validate_params(&schema, &params(&[("k", "a".into())])).is_ok());
    }

    #[test]
    fn test_validate_ok_when_optional_absent() {
        let schema = parse_schema("station:\n  type: string\n").unwrap();
        assert!(validate_params(&schema, &params(&[])).is_ok());
    }
}
