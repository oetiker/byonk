#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenRepoState {
    Ready,
    Fetching,
    Error,
    Offline,
}

#[derive(Debug, Clone, Default)]
pub struct ScreenRepoStatus {
    pub state: Option<ScreenRepoState>,
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
            serde_json::to_string(&ScreenRepoState::Offline).unwrap(),
            "\"offline\""
        );
        assert_eq!(
            serde_json::to_string(&ScreenRepoState::Fetching).unwrap(),
            "\"fetching\""
        );
    }

    #[test]
    fn test_default_status_is_empty() {
        let s = ScreenRepoStatus::default();
        assert!(s.state.is_none() && s.resolved_sha.is_none() && s.error.is_none());
    }
}
