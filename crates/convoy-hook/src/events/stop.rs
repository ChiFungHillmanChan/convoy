use convoy_core::project_id_from_cwd;
use convoy_daemon::rpc::Request;
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let (session_id, _payload) = crate::read_hook_input()?;
    let project_id = project_id_from_cwd()?;
    let client = DaemonClient::connect(&crate::default_socket()).await?;
    let _ = client.call_project(project_id, Request::EndSession { id: session_id }).await?;
    Ok(())
}
