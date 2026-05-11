use convoy_core::project_id_from_cwd;
use convoy_daemon::rpc::{Request, Response};
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let session_id = crate::read_session_id()?;
    let project_id = project_id_from_cwd()?;
    let client = DaemonClient::connect(&crate::default_socket()).await?;
    let _ = client.call_project(project_id.clone(), Request::Heartbeat { id: session_id.clone() }).await?;

    // Unread mail
    if let Response::Inbox { messages } = client
        .call_project(project_id, Request::ReadInbox {
            id: session_id.clone(),
            unread_only: true,
            limit: 10,
        })
        .await?
    {
        if !messages.is_empty() {
            let lines: Vec<String> = messages.iter().filter_map(|m| {
                let from = m.get("from")?.as_str()?;
                let kind = m.get("kind")?.as_str()?;
                let body = m.get("body")?.as_str()?;
                Some(format!("  [{}] {}: {}", kind, from, body))
            }).collect();
            crate::inject::emit(&format!("unread mail ({}):\n{}", messages.len(), lines.join("\n")))?;
        }
    }
    Ok(())
}
