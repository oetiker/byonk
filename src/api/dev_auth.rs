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
}
