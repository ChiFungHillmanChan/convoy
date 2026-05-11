//! Acceptance scenarios §15.1–§15.5.

mod common;
use common::harness::TwoSessionHarness;

use convoy_core::{MessageKind, Nickname, SessionId, WaitCondition};
use convoy_daemon::{Request, Response};
use convoy_store::Store;
use std::path::PathBuf;

fn make_session_id() -> SessionId {
    SessionId::new()
}

fn make_nickname(s: &str) -> String {
    s.into()
}

async fn register_session(
    harness: &TwoSessionHarness,
    use_a: bool,
    id: SessionId,
    nickname: &str,
) -> anyhow::Result<()> {
    let req = Request::RegisterSession {
        id: id.clone(),
        agent_tag: "claude-code".into(),
        pid: std::process::id(),
        nickname: nickname.into(),
        branch: Some("main".into()),
        worktree_path: None,
    };
    let resp = if use_a {
        harness.client_a.call(req).await?
    } else {
        harness.client_b.call(req).await?
    };
    assert!(matches!(resp, Response::Ok), "register failed: {resp:?}");
    Ok(())
}

/// §15.1 Discover each other: register two sessions, listSessions returns both.
#[tokio::test]
async fn test_15_1_discover_each_other() -> anyhow::Result<()> {
    let h = TwoSessionHarness::new().await?;
    let id_a = make_session_id();
    let id_b = make_session_id();

    register_session(&h, true, id_a.clone(), "alpha").await?;
    register_session(&h, false, id_b.clone(), "beta").await?;

    let resp = h.client_a.call(Request::ListSessions { include_ended: false }).await?;
    match resp {
        Response::Sessions { sessions } => {
            assert_eq!(sessions.len(), 2, "expected 2 active sessions, got {}", sessions.len());
            // Both session IDs appear in the list.
            let ids_json = serde_json::to_string(&sessions)?;
            assert!(ids_json.contains(&id_a.to_string()), "session A missing from list");
            assert!(ids_json.contains(&id_b.to_string()), "session B missing from list");
        }
        other => panic!("expected Sessions response, got {other:?}"),
    }

    // Verify via store directly.
    let active = h.store.list_active_sessions().await?;
    assert_eq!(active.len(), 2);
    Ok(())
}

/// §15.2 Lock conflict: A claims X.rs, B's claim returns claimed=false with held_by=A.
#[tokio::test]
async fn test_15_2_lock_conflict() -> anyhow::Result<()> {
    let h = TwoSessionHarness::new().await?;
    let id_a = make_session_id();
    let id_b = make_session_id();

    register_session(&h, true, id_a.clone(), "alpha").await?;
    register_session(&h, false, id_b.clone(), "beta").await?;

    let path: PathBuf = "/tmp/test-convoy-x.rs".into();

    // A claims the file.
    let claim_a = h.client_a.call(Request::ClaimFile {
        session: id_a.clone(),
        abs_path: path.clone(),
        reason: Some("refactoring".into()),
        ttl_sec: 300,
    }).await?;
    assert!(
        matches!(claim_a, Response::ClaimResult { claimed: true, .. }),
        "A should have claimed: {claim_a:?}"
    );

    // B tries to claim the same file.
    let claim_b = h.client_b.call(Request::ClaimFile {
        session: id_b.clone(),
        abs_path: path.clone(),
        reason: None,
        ttl_sec: 300,
    }).await?;
    match claim_b {
        Response::ClaimResult { claimed: false, held_by: Some(ref holder), .. } => {
            assert_eq!(*holder, id_a, "held_by should be A's session id");
        }
        other => panic!("expected ClaimResult with claimed=false, held_by=A, got {other:?}"),
    }

    // Verify lock is in the store.
    let lock = h.store.lock_for(&path).await?;
    assert!(lock.is_some(), "lock should exist in store");
    assert_eq!(lock.unwrap().session_id, id_a);
    Ok(())
}

/// §15.3 Status visibility: A updates status, B's `latest_status(A)` returns it.
#[tokio::test]
async fn test_15_3_status_visibility() -> anyhow::Result<()> {
    let h = TwoSessionHarness::new().await?;
    let id_a = make_session_id();
    let id_b = make_session_id();

    register_session(&h, true, id_a.clone(), "alpha").await?;
    register_session(&h, false, id_b.clone(), "beta").await?;

    // A pushes a status update.
    let summary = "done with login refactor".to_string();
    let resp = h.client_a.call(Request::UpdateStatus {
        id: id_a.clone(),
        summary: summary.clone(),
    }).await?;
    assert!(matches!(resp, Response::Ok));

    // Verify via store directly (as B would see via context injection).
    let st = h.store.latest_status(&id_a).await?;
    assert_eq!(st, Some(summary), "status should be visible via store");
    Ok(())
}

/// §15.4 Send/receive: A sends question to B, B's inbox has it.
#[tokio::test]
async fn test_15_4_send_receive() -> anyhow::Result<()> {
    let h = TwoSessionHarness::new().await?;
    let id_a = make_session_id();
    let id_b = make_session_id();

    register_session(&h, true, id_a.clone(), "alpha").await?;
    register_session(&h, false, id_b.clone(), "beta").await?;

    let body = "Are you done with config.yaml?".to_string();

    // A sends a question to B.
    let resp = h.client_a.call(Request::SendMessage {
        from: id_a.clone(),
        to: Some(id_b.clone()),
        kind: MessageKind::Question,
        in_reply_to: None,
        body: body.clone(),
    }).await?;
    assert!(matches!(resp, Response::MessageCreated { .. }), "send failed: {resp:?}");

    // B reads inbox.
    let inbox_resp = h.client_b.call(Request::ReadInbox {
        id: id_b.clone(),
        unread_only: false,
        limit: 10,
    }).await?;
    match inbox_resp {
        Response::Inbox { messages } => {
            assert!(!messages.is_empty(), "B's inbox should have at least 1 message");
            let inbox_json = serde_json::to_string(&messages)?;
            assert!(inbox_json.contains(&body), "B's inbox missing message body");
        }
        other => panic!("expected Inbox response, got {other:?}"),
    }

    // Verify via store.
    let msgs = h.store.inbox(&id_b, false, 10).await?;
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].from, id_a);
    assert_eq!(msgs[0].body, body);
    Ok(())
}

/// §15.5 Wait satisfied by release: B wait_for(LockReleased, X.rs) → A releases → B's wait satisfied.
#[tokio::test]
async fn test_15_5_wait_satisfied_by_release() -> anyhow::Result<()> {
    let h = TwoSessionHarness::new().await?;
    let id_a = make_session_id();
    let id_b = make_session_id();

    register_session(&h, true, id_a.clone(), "alpha").await?;
    register_session(&h, false, id_b.clone(), "beta").await?;

    let path: PathBuf = "/tmp/test-convoy-wait.rs".into();

    // A claims the file.
    h.client_a.call(Request::ClaimFile {
        session: id_a.clone(),
        abs_path: path.clone(),
        reason: Some("rewrite".into()),
        ttl_sec: 300,
    }).await?;

    // B registers a wait for the lock to be released.
    let wait_resp = h.client_b.call(Request::WaitFor {
        session: id_b.clone(),
        condition: WaitCondition::LockReleased { abs_path: path.clone() },
        timeout_sec: 30,
        hint: None,
    }).await?;
    let wait_id = match wait_resp {
        Response::WaitCreated { wait_id, status } => {
            assert_eq!(status, "waiting", "wait should be pending");
            wait_id
        }
        other => panic!("expected WaitCreated, got {other:?}"),
    };

    // A releases the file.
    let release_resp = h.client_a.call(Request::ReleaseFile {
        session: id_a.clone(),
        abs_path: path.clone(),
    }).await?;
    assert!(matches!(release_resp, Response::Ok));

    // Give the notifier a moment to fire (it's async within the server task).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // B's wait should now be satisfied.
    let waits = h.store.pending_waits().await?;
    let still_pending = waits.iter().any(|w| w.id == wait_id);
    assert!(!still_pending, "wait should no longer be pending after lock released");

    // Verify the wait record has satisfied_at set.
    // We check via pending_waits being empty of this id (satisfied waits are removed from pending).
    // Additionally verify the lock is gone.
    let lock = h.store.lock_for(&path).await?;
    assert!(lock.is_none(), "lock should be released");
    Ok(())
}
