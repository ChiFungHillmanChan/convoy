//! Project identification — derives a stable id from a filesystem path.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;

/// 12-hex-char project identifier derived from canonical path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

impl ProjectId {
    /// Derive a [`ProjectId`] from any string view of an already-canonicalized path.
    ///
    /// Caller is responsible for resolving symlinks / git-common-dir; this
    /// function is pure and only hashes.
    pub fn from_canonical_path(path: &Path) -> Self {
        let bytes = path.as_os_str().to_string_lossy();
        let mut h = Sha256::new();
        h.update(bytes.as_bytes());
        let digest = h.finalize();
        Self(hex::encode(&digest[..6])) // 6 bytes = 12 hex chars
    }

    /// Borrow the 12-char hex view.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Derive a [`ProjectId`] from the current working directory.
///
/// For git repositories, resolves to the common git dir (handles worktrees
/// by calling `git rev-parse --git-common-dir`). Falls back to a plain
/// `std::fs::canonicalize` of cwd on non-git directories.
pub fn project_id_from_cwd() -> anyhow::Result<ProjectId> {
    let cwd = std::env::current_dir()?;
    // Try git common dir first (so worktrees hash to the same project).
    if let Some(common) = git_common_dir(&cwd) {
        // The common dir ends in /.git or similar; we want the worktree root.
        // Walk up to find the working tree root: parent of .git dir.
        let root = if common.ends_with(".git") {
            common.parent().map(|p| p.to_path_buf()).unwrap_or(common)
        } else {
            common
        };
        let canonical = std::fs::canonicalize(&root).unwrap_or(root);
        return Ok(ProjectId::from_canonical_path(&canonical));
    }
    let canonical = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    Ok(ProjectId::from_canonical_path(&canonical))
}

fn git_common_dir(cwd: &Path) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "--git-common-dir" {
        return None;
    }
    // May be relative to cwd
    let p = std::path::PathBuf::from(&s);
    if p.is_absolute() {
        Some(p)
    } else {
        Some(cwd.join(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn id_is_twelve_hex_chars() {
        let id = ProjectId::from_canonical_path(Path::new("/tmp/foo"));
        assert_eq!(id.as_str().len(), 12);
        assert!(id.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn same_path_gives_same_id() {
        let a = ProjectId::from_canonical_path(Path::new("/Users/foo/repo"));
        let b = ProjectId::from_canonical_path(&PathBuf::from("/Users/foo/repo"));
        assert_eq!(a, b);
    }

    #[test]
    fn different_paths_give_different_ids() {
        let a = ProjectId::from_canonical_path(Path::new("/Users/foo/a"));
        let b = ProjectId::from_canonical_path(Path::new("/Users/foo/b"));
        assert_ne!(a, b);
    }
}
