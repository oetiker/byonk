#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageState {
    Ready,
    Fetching,
    Error,
    Offline,
}

#[derive(Debug, Clone, Default)]
pub struct PackageStatus {
    pub state: Option<PackageState>,
    pub resolved_sha: Option<String>,
    pub last_fetched: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
    pub pin_kind: Option<crate::services::git_fetch::PinKind>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&PackageState::Offline).unwrap(),
            "\"offline\""
        );
        assert_eq!(
            serde_json::to_string(&PackageState::Fetching).unwrap(),
            "\"fetching\""
        );
    }

    #[test]
    fn test_default_status_is_empty() {
        let s = PackageStatus::default();
        assert!(s.state.is_none() && s.resolved_sha.is_none() && s.error.is_none());
    }
}
