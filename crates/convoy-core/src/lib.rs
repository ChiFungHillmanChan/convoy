//! Pure domain types and rules for Convoy. No I/O.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ids;
pub mod project;
pub mod lock;
pub mod message;
pub mod wait;
pub mod rate_limit;

pub use ids::{Nickname, NicknameError, SessionId};
pub use project::ProjectId;
pub use lock::FileLock;
pub use message::{Message, MessageId, MessageKind};
pub use wait::{Event, WaitCondition};
pub use rate_limit::RateLimiter;

/// Convoy crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!VERSION.is_empty());
    }
}
