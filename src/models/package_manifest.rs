//! `byonk-screens.yaml` — the mandatory package manifest at a package root.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    #[serde(default)]
    pub root: Option<String>,
}

impl PackageManifest {
    pub fn from_yaml(src: &str) -> Result<PackageManifest, String> {
        serde_yaml::from_str(src).map_err(|e| format!("invalid byonk-screens.yaml: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let src = "name: acme\ndescription: d\nauthor: a\nlicense: MIT\nroot: contrib/trmnl\n";
        let m = PackageManifest::from_yaml(src).unwrap();
        assert_eq!(m.name, "acme");
        assert_eq!(m.root.as_deref(), Some("contrib/trmnl"));
    }

    #[test]
    fn test_root_optional() {
        let src = "name: a\ndescription: d\nauthor: x\nlicense: MIT\n";
        assert_eq!(PackageManifest::from_yaml(src).unwrap().root, None);
    }

    #[test]
    fn test_missing_required_field_errors() {
        let src = "name: a\ndescription: d\n"; // no author/license
        assert!(PackageManifest::from_yaml(src).is_err());
    }
}
