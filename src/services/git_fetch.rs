//! Low-level git primitive for screen-package distribution.
//!
//! [`fetch`] clones/fetches a package repo at a *pin* (a full sha, tag, or
//! branch name) and materializes the pinned tree into a plain directory —
//! not a git working tree. We clone bare into scratch space, resolve `pin`
//! against the fetched refs/objects, then export the resolved tree's blobs
//! directly into `dest`. That keeps `dest` a clean content directory with no
//! `.git` folder mixed in, which matters because later code (`PackageSource`
//! disk-walking in `package_loader.rs`) walks `dest` for package files and
//! shouldn't have to know about, or skip over, git internals.
//!
//! This interface is deliberately engine-agnostic (no `gix` types appear in
//! the public signatures below) so a later swap to `git2` — should `gix`
//! prove unworkable for some repo/auth shape — only touches this file.

use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;

/// How a `pin` string passed to [`fetch`] was resolved to a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PinKind {
    /// A full (40 hex char) commit sha.
    Sha,
    /// A tag name (`refs/tags/<pin>`).
    Tag,
    /// A branch name (`refs/heads/<pin>` on the remote).
    Branch,
}

/// The result of a successful [`fetch`].
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    /// The full commit sha `pin` resolved to.
    pub resolved_sha: String,
    /// How `pin` was classified/resolved.
    pub pin_kind: PinKind,
}

/// Errors from [`fetch`].
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// Any lower-level git failure (network, protocol, io, object lookup).
    #[error("git error: {0}")]
    Git(String),
    /// `pin` didn't resolve to a sha, tag, or branch in `repo`.
    #[error("pin `{0}` not found in {1}")]
    PinNotFound(String, String),
}

/// Classify a pin without network access: 40 lowercase-or-uppercase hex
/// chars is treated as an obvious full sha. Anything else (tags, branches,
/// short shas) is left for [`fetch`] to resolve against the remote.
pub fn looks_like_sha(pin: &str) -> bool {
    pin.len() == 40 && pin.chars().all(|c| c.is_ascii_hexdigit())
}

/// Name we fetch under; we never override it, so this matches what `gix`
/// would otherwise default to on its own.
const REMOTE_NAME: &str = "origin";

/// Clone/fetch `repo` at `pin`, materialize a working tree at `dest`
/// (created fresh), and return the resolved sha + pin kind. `token`, when
/// present, is used for HTTPS auth (GitHub convention: username
/// `x-access-token`, token as password); otherwise ambient host git creds
/// (credential helpers, `~/.gitconfig`) apply.
pub fn fetch(
    repo: &str,
    pin: &str,
    token: Option<&str>,
    dest: &Path,
) -> Result<FetchOutcome, FetchError> {
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .map_err(|e| FetchError::Git(format!("removing existing {}: {e}", dest.display())))?;
    }
    std::fs::create_dir_all(dest)
        .map_err(|e| FetchError::Git(format!("creating {}: {e}", dest.display())))?;

    let scratch = scratch_dir();
    let result = fetch_into(repo, pin, token, dest, &scratch);
    // Always clean up scratch, regardless of outcome.
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn scratch_dir() -> PathBuf {
    let unique = format!(
        "byonk-git-fetch-{}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        rand::random::<u64>()
    );
    std::env::temp_dir().join(unique)
}

fn fetch_into(
    repo: &str,
    pin: &str,
    token: Option<&str>,
    dest: &Path,
    scratch: &Path,
) -> Result<FetchOutcome, FetchError> {
    let fetch_url = auth_url(repo, token);

    let mut prep = gix::prepare_clone_bare(fetch_url.as_str(), scratch)
        .map_err(|e| FetchError::Git(format!("preparing clone of {repo}: {e}")))?;

    let interrupt = std::sync::atomic::AtomicBool::new(false);
    let (git_repo, _outcome) = prep
        .fetch_only(gix::progress::Discard, &interrupt)
        .map_err(|e| FetchError::Git(format!("fetching {repo}: {e}")))?;

    let (oid, pin_kind) = resolve_pin(&git_repo, pin)
        .ok_or_else(|| FetchError::PinNotFound(pin.to_string(), repo.to_string()))?;

    export_tree(&git_repo, oid, dest)
        .map_err(|e| FetchError::Git(format!("checking out {pin} ({oid}): {e}")))?;

    Ok(FetchOutcome {
        resolved_sha: oid.to_string(),
        pin_kind,
    })
}

/// Embed `token` as HTTPS basic-auth (GitHub's `x-access-token` convention)
/// when `repo` is an `https://` URL and a token was given. Left as-is
/// otherwise (including for `file://`/plain-path URLs used in tests, and for
/// `ssh://`/scp-style URLs where ambient host git/ssh creds apply).
fn auth_url(repo: &str, token: Option<&str>) -> String {
    match (repo.strip_prefix("https://"), token) {
        (Some(rest), Some(t)) if !t.is_empty() => {
            let encoded =
                percent_encoding::utf8_percent_encode(t, percent_encoding::NON_ALPHANUMERIC);
            format!("https://x-access-token:{encoded}@{rest}")
        }
        _ => repo.to_string(),
    }
}

/// Resolve `pin` against an already-fetched repo, trying (in order): a full
/// sha as an object id, a tag ref, a branch ref (remote-tracking, since we
/// never check out a local branch).
fn resolve_pin(repo: &gix::Repository, pin: &str) -> Option<(gix::ObjectId, PinKind)> {
    if looks_like_sha(pin) {
        if let Ok(oid) = gix::ObjectId::from_hex(pin.as_bytes()) {
            if repo.find_object(oid).is_ok() {
                return Some((oid, PinKind::Sha));
            }
        }
    }
    if let Ok(mut r) = repo.find_reference(format!("refs/tags/{pin}").as_str()) {
        if let Ok(id) = r.peel_to_id_in_place() {
            return Some((id.detach(), PinKind::Tag));
        }
    }
    if let Ok(mut r) = repo.find_reference(format!("refs/remotes/{REMOTE_NAME}/{pin}").as_str()) {
        if let Ok(id) = r.peel_to_id_in_place() {
            return Some((id.detach(), PinKind::Branch));
        }
    }
    None
}

/// Write every blob reachable from `commit_id`'s tree into `dest`, preserving
/// relative paths and the executable bit. Submodules (gitlink entries) are
/// skipped — not needed for screen packages. Symlinks are written as regular
/// files containing the link target text (best-effort; screen packages are
/// not expected to contain symlinks).
fn export_tree(
    repo: &gix::Repository,
    commit_id: gix::ObjectId,
    dest: &Path,
) -> Result<(), String> {
    let tree = repo
        .find_object(commit_id)
        .map_err(|e| e.to_string())?
        .peel_to_tree()
        .map_err(|e| e.to_string())?;
    let entries = tree
        .traverse()
        .breadthfirst
        .files()
        .map_err(|e| e.to_string())?;

    for entry in entries {
        if entry.mode.is_commit() {
            continue; // submodule gitlink: not supported
        }
        let rel = entry
            .filepath
            .to_str()
            .map_err(|e| format!("non-utf8 path {}: {e}", entry.filepath))?;
        let path = dest.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let blob = repo.find_object(entry.oid).map_err(|e| e.to_string())?;
        std::fs::write(&path, &blob.data).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        if entry.mode.is_executable() {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions();
            perm.set_mode(perm.mode() | 0o111);
            std::fs::set_permissions(&path, perm).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_sha() {
        assert!(looks_like_sha("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0"));
        assert!(!looks_like_sha("v1.0.0"));
        assert!(!looks_like_sha("main"));
        assert!(!looks_like_sha("a1b2c3d")); // short sha not treated as full sha
    }

    /// A source fixture repo built on disk with `gix` (init + one commit on
    /// the default branch + a tag), used as a hermetic `fetch()` source via
    /// a plain filesystem path (no network).
    struct FixtureRepo {
        url: String,
        branch: String,
        tag: String,
        head_sha: String,
    }

    fn tempdir_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "byonk-{prefix}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    /// Build a real git repo on disk: init, write `byonk-screens.yaml`,
    /// commit on the default branch, tag it `v1`.
    fn make_fixture_repo() -> FixtureRepo {
        let dir = tempdir_path("git_fetch_src");
        std::fs::create_dir_all(&dir).expect("create fixture dir");

        let mut repo = gix::init(&dir).expect("init fixture repo");

        std::fs::write(dir.join("byonk-screens.yaml"), b"root: .\n")
            .expect("write fixture manifest");

        // Stage the file into a tree.
        let blob_id = repo
            .write_blob(std::fs::read(dir.join("byonk-screens.yaml")).unwrap())
            .expect("write blob")
            .detach();
        let tree_id = {
            let mut tree = gix::objs::Tree::empty();
            tree.entries.push(gix::objs::tree::Entry {
                mode: gix::objs::tree::EntryKind::Blob.into(),
                filename: "byonk-screens.yaml".into(),
                oid: blob_id,
            });
            repo.write_object(&tree).expect("write tree").detach()
        };

        // Ensure a committer/author identity is available even in a bare CI
        // environment without any git config.
        let mut config = repo.config_snapshot_mut();
        config
            .set_raw_value(&gix::config::tree::User::NAME, "byonk-test")
            .expect("set user.name");
        config
            .set_raw_value(&gix::config::tree::User::EMAIL, "byonk-test@example.com")
            .expect("set user.email");
        config.commit().expect("commit config");

        let commit_id = repo
            .commit(
                "HEAD",
                "initial commit",
                tree_id,
                std::iter::empty::<gix::ObjectId>(),
            )
            .expect("commit fixture");
        let head_sha = commit_id.to_string();

        let branch = repo
            .head_name()
            .expect("read head")
            .and_then(|n| n.shorten().to_str().ok().map(|s| s.to_string()))
            .unwrap_or_else(|| "main".to_string());

        let tag = "v1".to_string();
        repo.tag_reference(
            &tag,
            commit_id,
            gix::refs::transaction::PreviousValue::MustNotExist,
        )
        .expect("create tag");

        FixtureRepo {
            url: dir.to_string_lossy().to_string(),
            branch,
            tag,
            head_sha,
        }
    }

    #[test]
    fn test_fetch_branch_from_local_repo() {
        let src = make_fixture_repo();
        let dest = tempdir_path("git_fetch_dest_branch");

        let out = fetch(&src.url, &src.branch, None, &dest).expect("fetch branch");

        assert_eq!(out.resolved_sha, src.head_sha);
        assert_eq!(out.pin_kind, PinKind::Branch);
        assert!(dest.join("byonk-screens.yaml").exists());

        std::fs::remove_dir_all(&src.url).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn test_fetch_tag_from_local_repo() {
        let src = make_fixture_repo();
        let dest = tempdir_path("git_fetch_dest_tag");

        let out = fetch(&src.url, &src.tag, None, &dest).expect("fetch tag");

        assert_eq!(out.resolved_sha, src.head_sha);
        assert_eq!(out.pin_kind, PinKind::Tag);
        assert!(dest.join("byonk-screens.yaml").exists());

        std::fs::remove_dir_all(&src.url).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn test_fetch_sha_from_local_repo() {
        let src = make_fixture_repo();
        let dest = tempdir_path("git_fetch_dest_sha");

        let out = fetch(&src.url, &src.head_sha, None, &dest).expect("fetch sha");

        assert_eq!(out.resolved_sha, src.head_sha);
        assert_eq!(out.pin_kind, PinKind::Sha);
        assert!(dest.join("byonk-screens.yaml").exists());

        std::fs::remove_dir_all(&src.url).ok();
        std::fs::remove_dir_all(&dest).ok();
    }

    #[test]
    fn test_fetch_missing_pin_errors() {
        let src = make_fixture_repo();
        let dest = tempdir_path("git_fetch_dest_missing");

        let err = fetch(&src.url, "no-such-ref", None, &dest).unwrap_err();
        assert!(matches!(err, FetchError::PinNotFound(_, _)));

        std::fs::remove_dir_all(&src.url).ok();
    }

    /// Network path against a real, small, stable public repo. Not run by
    /// default (`cargo test` / CI have no network); run explicitly with
    /// `cargo test -p byonk git_fetch -- --ignored`.
    #[test]
    #[ignore]
    fn test_fetch_network_public_repo() {
        let dest = tempdir_path("git_fetch_dest_network");
        let out = fetch(
            "https://github.com/octocat/Hello-World.git",
            "master",
            None,
            &dest,
        )
        .expect("fetch from network");
        assert_eq!(out.pin_kind, PinKind::Branch);
        assert!(!out.resolved_sha.is_empty());
        std::fs::remove_dir_all(&dest).ok();
    }
}
