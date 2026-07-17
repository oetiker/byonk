use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub struct ScreenRepoCache {
    root: PathBuf,
}

fn repo_key(repo: &str) -> String {
    let mut h = Sha256::new();
    h.update(repo.as_bytes());
    hex::encode(&h.finalize()[..8]) // 16 hex chars — enough to avoid collisions
}

impl ScreenRepoCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Directory a given repo+sha checkout lives at: root/<repo_hash>/<sha>.
    pub fn checkout_dir(&self, repo: &str, sha: &str) -> PathBuf {
        self.root.join(repo_key(repo)).join(sha)
    }

    /// True if that checkout already exists on disk (and has a manifest).
    pub fn has(&self, repo: &str, sha: &str) -> bool {
        self.checkout_dir(repo, sha)
            .join("byonk-screens.yaml")
            .exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkout_dir_is_stable_and_scoped() {
        let c = ScreenRepoCache::new(std::path::PathBuf::from("/tmp/byonk-cache"));
        let a = c.checkout_dir("github.com/acme/x", "deadbeef");
        let b = c.checkout_dir("github.com/acme/x", "deadbeef");
        let d = c.checkout_dir("github.com/acme/y", "deadbeef");
        assert_eq!(a, b); // stable
        assert_ne!(a, d); // different repo ⇒ different dir
        assert!(a.starts_with("/tmp/byonk-cache"));
        assert!(a.ends_with("deadbeef"));
    }

    #[test]
    fn test_has_false_when_absent() {
        let c = ScreenRepoCache::new(std::env::temp_dir().join("byonk-cache-none"));
        assert!(!c.has("github.com/acme/x", "deadbeef"));
    }
}
