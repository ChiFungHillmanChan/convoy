#![forbid(unsafe_code)]
pub mod inject;
pub mod events;

pub fn read_session_id() -> anyhow::Result<convoy_core::SessionId> {
    std::env::var("CLAUDE_SESSION_ID")
        .map(convoy_core::SessionId::from_string_unchecked)
        .map_err(|e| anyhow::anyhow!(e))
}

pub fn default_socket() -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "{}/.convoy/daemon.sock",
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
    ))
}
