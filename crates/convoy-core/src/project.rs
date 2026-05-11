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
