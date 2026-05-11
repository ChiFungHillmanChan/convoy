//! Client side of the daemon RPC.

use crate::rpc::{Envelope, Request, Response};
use convoy_core::ProjectId;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

pub struct DaemonClient {
    stream: Mutex<UnixStream>,
}

impl DaemonClient {
    pub async fn connect(sock: &Path) -> anyhow::Result<Self> {
        Ok(Self { stream: Mutex::new(UnixStream::connect(sock).await?) })
    }

    /// Send a request routed to the given project.
    pub async fn call_project(&self, project_id: ProjectId, req: Request) -> anyhow::Result<Response> {
        let env = Envelope { project_id, op: req };
        let mut s = self.stream.lock().await;
        let line = serde_json::to_string(&env)? + "\n";
        s.write_all(line.as_bytes()).await?;
        let (rx, _) = s.split();
        let mut reader = BufReader::new(rx);
        let mut buf = String::new();
        reader.read_line(&mut buf).await?;
        let resp: Response = serde_json::from_str(buf.trim())?;
        Ok(resp)
    }

    /// Send a bare request without project routing (backward-compat for tests).
    pub async fn call(&self, req: Request) -> anyhow::Result<Response> {
        let mut s = self.stream.lock().await;
        let line = serde_json::to_string(&req)? + "\n";
        s.write_all(line.as_bytes()).await?;
        let (rx, _) = s.split();
        let mut reader = BufReader::new(rx);
        let mut buf = String::new();
        reader.read_line(&mut buf).await?;
        let resp: Response = serde_json::from_str(buf.trim())?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        notify::Notifier,
        server::Daemon,
    };
    use convoy_store::MemoryStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn client_ping_pong() {
        let dir = tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let store = Arc::new(MemoryStore::new());
        let notifier = Notifier::new(store.clone());
        let daemon = Arc::new(Daemon::new(store, notifier));

        let sock_clone = sock.clone();
        tokio::spawn(async move {
            daemon.serve(sock_clone.as_path()).await.unwrap();
        });

        // Small delay for the socket to appear
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = DaemonClient::connect(&sock).await.unwrap();
        let resp = client.call(Request::Ping).await.unwrap();
        assert!(matches!(resp, Response::Pong));
    }
}
