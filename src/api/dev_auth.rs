//! Dev UI authentication: credentials, session cookies, and the
//! `require_dev` middleware with its three admission paths.

/// One username + password pair for the form login.
#[derive(Debug, Clone)]
pub struct DevCredentials {
    pub username: String,
    pub password: String,
}

/// Everything `require_dev` needs. Lives on `AppState` as `Arc<DevAuth>`.
#[derive(Debug)]
pub struct DevAuth {
    pub credentials: Option<DevCredentials>,
    /// Random per process: a restart logs everyone out.
    pub session_key: [u8; 32],
}

fn fresh_key() -> [u8; 32] {
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn non_blank(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub const COOKIE_NAME: &str = "byonk_dev";
pub const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 3600;

fn mac_hex(key: &[u8; 32], issued: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(issued.to_string().as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl DevAuth {
    /// No form login. Ingress and bearer paths still work.
    pub fn disabled() -> Self {
        Self {
            credentials: None,
            session_key: fresh_key(),
        }
    }

    pub fn with_credentials(username: &str, password: &str) -> Self {
        Self {
            credentials: Some(DevCredentials {
                username: username.to_string(),
                password: password.to_string(),
            }),
            session_key: fresh_key(),
        }
    }

    /// Env wins over config (which the add-on options already wrote into);
    /// blank counts as unset; both halves are required.
    pub fn resolve(config_user: Option<&str>, config_pass: Option<&str>) -> Self {
        let user = non_blank(std::env::var("BYONK_DEV_USERNAME").ok().as_deref())
            .or_else(|| non_blank(config_user));
        let pass = non_blank(std::env::var("BYONK_DEV_PASSWORD").ok().as_deref())
            .or_else(|| non_blank(config_pass));
        match (user, pass) {
            (Some(u), Some(p)) => Self::with_credentials(&u, &p),
            _ => Self::disabled(),
        }
    }

    /// `<issued_unix>.<hex hmac>`; `now` is injected for tests.
    pub fn issue_cookie_value(&self, now: i64) -> String {
        format!("{now}.{}", mac_hex(&self.session_key, now))
    }

    pub fn verify_cookie_value(&self, value: &str, now: i64) -> bool {
        let Some((issued, mac)) = value.split_once('.') else {
            return false;
        };
        let Ok(issued) = issued.parse::<i64>() else {
            return false;
        };
        if now.saturating_sub(issued) > SESSION_MAX_AGE_SECS || issued > now.saturating_add(60) {
            return false;
        }
        crate::api::admin::constant_time_eq(
            mac.as_bytes(),
            mac_hex(&self.session_key, issued).as_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_requires_both_halves() {
        assert!(DevAuth::resolve(Some("u"), None).credentials.is_none());
        assert!(DevAuth::resolve(None, Some("p")).credentials.is_none());
        assert!(DevAuth::resolve(Some("u"), Some("p")).credentials.is_some());
    }

    #[test]
    fn resolve_treats_blank_as_unset() {
        assert!(DevAuth::resolve(Some("  "), Some("p"))
            .credentials
            .is_none());
    }

    #[test]
    fn two_instances_have_different_session_keys() {
        assert_ne!(
            DevAuth::disabled().session_key,
            DevAuth::disabled().session_key
        );
    }

    #[test]
    fn cookie_round_trips() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        assert!(a.verify_cookie_value(&v, 1_000 + 60));
    }

    #[test]
    fn cookie_from_another_key_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let b = DevAuth::with_credentials("u", "p");
        assert!(!b.verify_cookie_value(&a.issue_cookie_value(1_000), 1_060));
    }

    #[test]
    fn tampered_timestamp_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        let (_, mac) = v.split_once('.').unwrap();
        assert!(!a.verify_cookie_value(&format!("2000.{mac}"), 2_060));
    }

    #[test]
    fn expired_cookie_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        let v = a.issue_cookie_value(1_000);
        assert!(!a.verify_cookie_value(&v, 1_000 + SESSION_MAX_AGE_SECS + 1));
        assert!(a.verify_cookie_value(&v, 1_000 + SESSION_MAX_AGE_SECS - 1));
    }

    #[test]
    fn garbage_is_rejected() {
        let a = DevAuth::with_credentials("u", "p");
        assert!(!a.verify_cookie_value("", 0));
        assert!(!a.verify_cookie_value("abc", 0));
        assert!(!a.verify_cookie_value("12.zz", 0));
    }

    #[test]
    fn adversarial_timestamp_is_rejected_not_panicking() {
        let a = DevAuth::with_credentials("u", "p");
        assert!(!a.verify_cookie_value(&format!("{}.deadbeef", i64::MIN), 0));
        assert!(!a.verify_cookie_value(&format!("{}.deadbeef", i64::MAX), 0));
    }
}
