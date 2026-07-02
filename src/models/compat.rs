//! Semver compatibility between the running byonk engine and a screen's
//! `byonk:` requirement. Bare versions are treated as caret (`^`), matching
//! Cargo. A mismatch produces a warning string; it never blocks rendering.

use semver::{Version, VersionReq};

/// Current engine version (from Cargo).
pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns `Some(warning)` if `engine` does not satisfy `req`, else `None`.
/// A bare `"0.15"` is parsed as `^0.15`. Malformed input fails soft: it warns.
pub fn compat_warning(engine: &str, req: &str) -> Option<String> {
    let version = match Version::parse(engine) {
        Ok(v) => v,
        Err(e) => return Some(format!("cannot parse engine version `{engine}`: {e}")),
    };
    // `VersionReq::parse` already treats a bare "0.15" as ^0.15.
    let requirement = match VersionReq::parse(req) {
        Ok(r) => r,
        Err(e) => return Some(format!("invalid byonk requirement `{req}`: {e}")),
    };
    if requirement.matches(&version) {
        None
    } else {
        Some(format!(
            "screen requires byonk `{req}` but this engine is {engine}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bare_version_is_caret_ok() {
        // engine 0.15.3 satisfies "0.15" (^0.15 => >=0.15.0, <0.16.0)
        assert_eq!(compat_warning("0.15.3", "0.15"), None);
    }

    #[test]
    fn test_bare_version_next_minor_warns_pre_1_0() {
        // 0.x: minor is the breaking boundary; 0.16.0 is outside ^0.15
        assert!(compat_warning("0.16.0", "0.15").is_some());
    }

    #[test]
    fn test_below_min_warns() {
        assert!(compat_warning("0.14.0", "0.15").is_some());
    }

    #[test]
    fn test_explicit_range_ok() {
        assert_eq!(compat_warning("0.15.0", ">=0.14, <0.17").is_none(), true);
    }

    #[test]
    fn test_bad_requirement_warns_not_panics() {
        assert!(compat_warning("0.15.0", "not-a-version").is_some());
    }
}
