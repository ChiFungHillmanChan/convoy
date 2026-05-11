use convoy_core::{Agent, FileLock, Message, MessageId, MessageKind, Nickname, SessionId};
use convoy_daemon::exports::{render_locks, render_mail, render_sessions};
use convoy_store::{MemoryStore, RegisterArgs, Store};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

fn make_store_arc() -> Arc<dyn Store> {
    Arc::new(MemoryStore::new())
}

#[tokio::test]
async fn exports_sessions_and_mail_and_locks() {
    let store = make_store_arc();
    let dir = tempdir().unwrap();
    let exports = dir.path();

    let sid = SessionId::new();
    store
        .register_session(
            RegisterArgs {
                id: sid.clone(),
                agent: Agent::ClaudeCode,
                pid: 12345,
                nickname: Nickname::new("test-session").unwrap(),
                branch: Some("main".into()),
                worktree_path: None,
            },
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    store
        .push_status(&sid, "working on tests".into(), chrono::Utc::now())
        .await
        .unwrap();

    // Add a lock
    store
        .claim_file(FileLock {
            abs_path: PathBuf::from("/project/src/main.rs"),
            session_id: sid.clone(),
            reason: Some("editing".into()),
            claimed_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        })
        .await
        .unwrap();

    // Add a message
    let other = SessionId::new();
    store
        .insert_message(Message {
            id: MessageId::new(),
            from: other.clone(),
            to: Some(sid.clone()),
            kind: MessageKind::Info,
            in_reply_to: None,
            body: "hello there".into(),
            created_at: chrono::Utc::now(),
            read_at: None,
        })
        .await
        .unwrap();

    // Render all three
    render_sessions(&store, exports).await.unwrap();
    render_mail(&store, exports).await.unwrap();
    render_locks(&store, exports).await.unwrap();

    let sessions_md = std::fs::read_to_string(exports.join("sessions.md")).unwrap();
    let mail_md = std::fs::read_to_string(exports.join("recent_mail.md")).unwrap();
    let locks_md = std::fs::read_to_string(exports.join("locks.md")).unwrap();

    // Use insta with timestamp+uuid filters
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[\.\d]*[+\-Z][^\s]*",
        "[TIMESTAMP]",
    );
    settings.add_filter(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        "[UUID]",
    );
    // short hex IDs (6 chars after # in sessions.md)
    settings.add_filter(r"#[0-9a-f]{6}", "#[SHORT]");
    // bare 6-char hex IDs in mail_md (from .short())
    settings.add_filter(r"\b[0-9a-f]{6}\b", "[SHORT6]");

    settings.bind(|| {
        insta::assert_snapshot!("sessions_md", sessions_md);
        insta::assert_snapshot!("mail_md", mail_md);
        insta::assert_snapshot!("locks_md", locks_md);
    });
}
