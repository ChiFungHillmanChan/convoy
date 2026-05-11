//! Two-session test harness: spins up a real in-memory daemon on a temp
//! UNIX socket and provides two pre-connected DaemonClient handles.

use convoy_daemon::{notify::Notifier, server::Daemon, DaemonClient};
use convoy_store::MemoryStore;
use std::sync::Arc;
use tempfile::TempDir;

pub struct TwoSessionHarness {
    /// In-memory store shared by the daemon (accessible for direct inspection).
    pub store: Arc<MemoryStore>,
    /// Client connected as session A.
    pub client_a: DaemonClient,
    /// Client connected as session B.
    pub client_b: DaemonClient,
    /// Temp dir holding the socket (kept alive for test lifetime).
    pub _tmp: TempDir,
}

impl TwoSessionHarness {
    /// Spawn an in-memory daemon on a temp socket, connect two clients.
    pub async fn new() -> anyhow::Result<Self> {
        let tmp = tempfile::tempdir()?;
        let sock = tmp.path().join("test.sock");

        let store = Arc::new(MemoryStore::new());
        let notifier = Notifier::new(store.clone() as Arc<dyn convoy_store::Store>);
        let daemon = Arc::new(Daemon::new(
            store.clone() as Arc<dyn convoy_store::Store>,
            notifier,
        ));

        let sock_clone = sock.clone();
        tokio::spawn(async move {
            let _ = daemon.serve(&sock_clone).await;
        });

        // Wait for socket to appear.
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            if sock.exists() {
                break;
            }
        }

        let client_a = DaemonClient::connect(&sock).await?;
        let client_b = DaemonClient::connect(&sock).await?;

        Ok(Self { store, client_a, client_b, _tmp: tmp })
    }
}
