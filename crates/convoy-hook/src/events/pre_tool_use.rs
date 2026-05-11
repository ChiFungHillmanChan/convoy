use convoy_core::project_id_from_cwd;
use convoy_daemon::rpc::{Request, Response};
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let session_id = crate::read_session_id()?;
    let payload: serde_json::Value = serde_json::from_reader(std::io::stdin())?;
    let project_id = project_id_from_cwd()?;

    let socket = crate::default_socket();
    let client = DaemonClient::connect(&socket).await?;

    // Heartbeat
    let _ = client.call_project(project_id.clone(), Request::Heartbeat { id: session_id.clone() }).await?;

    // If tool is Write/Edit/NotebookEdit and the path is locked by someone else,
    // surface a warning.
    let tool = payload.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
    if matches!(tool, "Write" | "Edit" | "NotebookEdit") {
        if let Some(path) = payload.pointer("/tool_input/file_path").and_then(|v| v.as_str()) {
            if let Response::Locks { locks } = client.call_project(project_id, Request::ListLocks).await? {
                if let Some(l) = locks.iter().find(|l| l.abs_path.as_os_str() == path) {
                    if l.session_id != session_id {
                        crate::inject::emit(&format!(
                            "advisory: {} is held by {} (reason: {}) until {}",
                            path,
                            l.session_id.short(),
                            l.reason.as_deref().unwrap_or(""),
                            l.expires_at.to_rfc3339()
                        ))?;
                    }
                }
            }
        }
    }
    Ok(())
}

