//! Identity types for sessions and projects.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable per-session UUID. Survives renames.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate a fresh random session id.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Hex string view.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// First 6 characters, for display.
    pub fn short(&self) -> &str {
        &self.0[..6]
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Human-readable session name. Validated on construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Nickname(String);

/// Errors when constructing a [`Nickname`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NicknameError {
    /// Empty after trim.
    #[error("nickname must be non-empty")]
    Empty,
    /// Too long.
    #[error("nickname must be at most 64 chars (was {0})")]
    TooLong(usize),
    /// Contains characters we forbid for filesystem / display safety.
    #[error("nickname contains forbidden character: {0:?}")]
    ForbiddenChar(char),
}

impl Nickname {
    /// Construct a nickname, validating the input.
    ///
    /// Rules: 1–64 chars after trim; allowed = ASCII letters, digits, '-', '_', '.'.
    pub fn new(s: impl AsRef<str>) -> Result<Self, NicknameError> {
        let t = s.as_ref().trim();
        if t.is_empty() {
            return Err(NicknameError::Empty);
        }
        if t.len() > 64 {
            return Err(NicknameError::TooLong(t.len()));
        }
        for c in t.chars() {
            let ok = c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
            if !ok {
                return Err(NicknameError::ForbiddenChar(c));
            }
        }
        Ok(Self(t.to_string()))
    }

    /// Borrow the validated string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Nickname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_short_is_six_chars() {
        let id = SessionId::new();
        assert_eq!(id.short().len(), 6);
        assert!(id.as_str().starts_with(id.short()));
    }

    #[test]
    fn session_ids_are_unique() {
        let a = SessionId::new();
        let b = SessionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn nickname_accepts_typical_names() {
        for n in ["feat-auth", "refactor_config", "v1.2", "crimson-otter"] {
            assert!(Nickname::new(n).is_ok(), "{n}");
        }
    }

    #[test]
    fn nickname_rejects_empty() {
        assert_eq!(Nickname::new("   "), Err(NicknameError::Empty));
    }

    #[test]
    fn nickname_rejects_too_long() {
        let s = "x".repeat(65);
        assert_eq!(Nickname::new(&s), Err(NicknameError::TooLong(65)));
    }

    #[test]
    fn nickname_rejects_forbidden_chars() {
        assert_eq!(Nickname::new("hi there"), Err(NicknameError::ForbiddenChar(' ')));
        assert_eq!(Nickname::new("emoji-not-ok-🚀"), Err(NicknameError::ForbiddenChar('🚀')));
    }
}
