//! Per-screen `meta.yaml`: title/description/compat/params — the single source
//! of screen truth. Parsed as YAML, never executed.

use serde::Deserialize;

use crate::models::param_schema::{parse_schema_from_value, ParamSchema};

#[derive(Debug, Clone)]
pub struct ScreenMeta {
    pub title: String,
    pub description: String,
    pub byonk: String,
    pub refresh: Option<u32>,
    pub params: ParamSchema,
}

#[derive(Deserialize)]
struct RawMeta {
    title: String,
    description: String,
    byonk: String,
    #[serde(default)]
    refresh: Option<u32>,
    #[serde(default)]
    params: serde_yaml::Value,
}

impl ScreenMeta {
    pub fn from_yaml(src: &str) -> Result<ScreenMeta, String> {
        let raw: RawMeta =
            serde_yaml::from_str(src).map_err(|e| format!("invalid meta.yaml: {e}"))?;
        let params = parse_schema_from_value(&raw.params)?;
        Ok(ScreenMeta {
            title: raw.title,
            description: raw.description,
            byonk: raw.byonk,
            refresh: raw.refresh,
            params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_meta() {
        let src = "title: 5-Day Forecast\ndescription: Daily conditions.\nbyonk: \"0.15\"\nrefresh: 900\nparams:\n  location:\n    type: string\n    required: true\n";
        let m = ScreenMeta::from_yaml(src).unwrap();
        assert_eq!(m.title, "5-Day Forecast");
        assert_eq!(m.description, "Daily conditions.");
        assert_eq!(m.byonk, "0.15");
        assert_eq!(m.refresh, Some(900));
        assert_eq!(m.params.fields.len(), 1);
    }

    #[test]
    fn test_missing_title_is_error() {
        let src = "description: x\nbyonk: \"0.15\"\n";
        assert!(ScreenMeta::from_yaml(src).is_err());
    }

    #[test]
    fn test_missing_byonk_is_error() {
        let src = "title: t\ndescription: d\n";
        assert!(ScreenMeta::from_yaml(src).is_err());
    }

    #[test]
    fn test_no_params_is_empty_schema() {
        let src = "title: t\ndescription: d\nbyonk: \"0.15\"\n";
        let m = ScreenMeta::from_yaml(src).unwrap();
        assert!(m.params.fields.is_empty());
        assert_eq!(m.refresh, None);
    }
}
