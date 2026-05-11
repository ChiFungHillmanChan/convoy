use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use convoy_daemon::{notify::Notifier, rpc::Request, server::Daemon};
use convoy_store::MemoryStore;

#[tokio::test]
async fn ping_pong() {
    let dir = tempdir().unwrap();
    let sock = dir.path().join("d.sock");
    let store = Arc::new(MemoryStore::new());
    let notifier = Notifier::new(store.clone());
    let daemon = Arc::new(Daemon::new(store, notifier));

    let sock_clone = sock.clone();
    let server = tokio::spawn(async move {
        daemon.serve(&sock_clone).await
    });

    // Give the listener a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut stream = UnixStream::connect(&sock).await.unwrap();
    let req = serde_json::to_string(&Request::Ping).unwrap() + "\n";
    stream.write_all(req.as_bytes()).await.unwrap();
    let (rx, _) = stream.split();
    let mut line = String::new();
    BufReader::new(rx).read_line(&mut line).await.unwrap();
    assert!(line.contains("pong"));

    server.abort();
}
