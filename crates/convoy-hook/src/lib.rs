#![forbid(unsafe_code)]
pub mod inject;
pub mod events;

/// Read the hook payload Claude Code sends on stdin.
///
/// Returns `(session_id, full_payload)`. The payload always includes at
/// minimum: `session_id`, `transcript_path`, `cwd`, `hook_event_name`. Event
/// specific fields (`tool_name`, `tool_input`, `prompt`, ...) are also in the
/// payload object.
///
/// Falls back to the `CLAUDE_SESSION_ID` env var if stdin has no payload —
/// this keeps the helper usable from manual smoke tests.
pub fn read_hook_input() -> anyhow::Result<(convoy_core::SessionId, serde_json::Value)> {
    // Read stdin if any is piped. If stdin is a TTY (no payload, e.g. manual
    // invocation), fall back to env var.
    use std::io::Read;
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    let payload: serde_json::Value = if buf.trim().is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        serde_json::from_str(&buf)
            .map_err(|e| anyhow::anyhow!("hook stdin is not valid JSON: {e}"))?
    };
    let from_payload = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let from_env = std::env::var("CLAUDE_SESSION_ID").ok();
    let session_id_str = from_payload.or(from_env).ok_or_else(|| {
        anyhow::anyhow!("session_id not found in hook stdin JSON or CLAUDE_SESSION_ID env")
    })?;
    Ok((
        convoy_core::SessionId::from_string_unchecked(session_id_str),
        payload,
    ))
}

/// Legacy helper kept for any caller that does not need the full payload.
pub fn read_session_id() -> anyhow::Result<convoy_core::SessionId> {
    read_hook_input().map(|(id, _)| id)
}

pub fn default_socket() -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "{}/.convoy/daemon.sock",
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
    ))
}
