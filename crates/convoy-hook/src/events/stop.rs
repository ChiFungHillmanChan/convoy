use convoy_daemon::rpc::Request;
use convoy_daemon::DaemonClient;

pub async fn run() -> anyhow::Result<()> {
    let session_id = crate::read_session_id()?;
    let client = DaemonClient::connect(&crate::default_socket()).await?;
    let _ = client.call(Request::EndSession { id: session_id }).await?;
    Ok(())
}
