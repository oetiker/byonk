//! Header parsing utilities for TRMNL API requests.

use axum::http::HeaderMap;

use crate::error::ApiError;

/// Extension trait for convenient header parsing.
pub trait HeaderMapExt {
    /// Get a header value as a string, or return an error if missing.
    fn require_str(&self, name: &'static str) -> Result<&str, ApiError>;

    /// Get a header value as a string, returning None if missing.
    fn get_str(&self, name: &str) -> Option<&str>;

    /// Get a header value parsed as a type, returning None if missing or invalid.
    fn get_parsed<T: std::str::FromStr>(&self, name: &str) -> Option<T>;

    /// Get a header value parsed as a type with a default if missing or invalid.
    fn get_parsed_or<T: std::str::FromStr>(&self, name: &str, default: T) -> T;

    /// Get a header value parsed as a type with validation.
    fn get_parsed_filtered<T, F>(&self, name: &str, filter: F) -> Option<T>
    where
        T: std::str::FromStr,
        F: FnOnce(&T) -> bool;
}

impl HeaderMapExt for HeaderMap {
    fn require_str(&self, name: &'static str) -> Result<&str, ApiError> {
        self.get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::MissingHeader(name))
    }

    fn get_str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.to_str().ok())
    }

    fn get_parsed<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
        self.get_str(name).and_then(|v| v.parse().ok())
    }

    fn get_parsed_or<T: std::str::FromStr>(&self, name: &str, default: T) -> T {
        self.get_parsed(name).unwrap_or(default)
    }

    fn get_parsed_filtered<T, F>(&self, name: &str, filter: F) -> Option<T>
    where
        T: std::str::FromStr,
        F: FnOnce(&T) -> bool,
    {
        self.get_parsed(name).filter(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    fn make_headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            // HTTP header names are case-insensitive
            let header_name = HeaderName::try_from(*name).unwrap();
            headers.insert(header_name, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn test_require_str_present() {
        let headers = make_headers(&[("id", "AA:BB:CC:DD:EE:FF")]);
        assert_eq!(headers.require_str("id").unwrap(), "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn test_require_str_missing() {
        let headers = HeaderMap::new();
        let err = headers.require_str("ID").unwrap_err();
        assert!(matches!(err, ApiError::MissingHeader("ID")));
    }

    #[test]
    fn test_get_str_present() {
        let headers = make_headers(&[("model", "og")]);
        assert_eq!(headers.get_str("model"), Some("og"));
    }

    #[test]
    fn test_get_str_missing() {
        let headers = HeaderMap::new();
        assert_eq!(headers.get_str("model"), None);
    }

    #[test]
    fn test_get_parsed_valid() {
        let headers = make_headers(&[("width", "800")]);
        assert_eq!(headers.get_parsed::<u32>("width"), Some(800));
    }

    #[test]
    fn test_get_parsed_invalid() {
        let headers = make_headers(&[("width", "not-a-number")]);
        assert_eq!(headers.get_parsed::<u32>("width"), None);
    }

    #[test]
    fn test_get_parsed_missing() {
        let headers = HeaderMap::new();
        assert_eq!(headers.get_parsed::<u32>("width"), None);
    }

    #[test]
    fn test_get_parsed_or_present() {
        let headers = make_headers(&[("width", "1200")]);
        assert_eq!(headers.get_parsed_or("width", 800u32), 1200);
    }

    #[test]
    fn test_get_parsed_or_missing() {
        let headers = HeaderMap::new();
        assert_eq!(headers.get_parsed_or("width", 800u32), 800);
    }

    #[test]
    fn test_get_parsed_filtered_valid() {
        let headers = make_headers(&[("width", "800")]);
        let result = headers.get_parsed_filtered::<u32, _>("width", |&w| w > 0 && w <= 2000);
        assert_eq!(result, Some(800));
    }

    #[test]
    fn test_get_parsed_filtered_invalid() {
        let headers = make_headers(&[("width", "9999")]);
        let result = headers.get_parsed_filtered::<u32, _>("width", |&w| w > 0 && w <= 2000);
        assert_eq!(result, None);
    }
}
