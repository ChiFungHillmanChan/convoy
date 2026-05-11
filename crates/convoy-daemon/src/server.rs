//! Daemon RPC server: accepts on a UNIX socket, dispatches to handlers.

use crate::notify::Notifier;
use crate::rpc::{Envelope, Request, Response};
use convoy_core::{FileLock, Message, MessageId, Nickname, ProjectId};
use convoy_store::{RegisterArgs, SqliteStore, Store, StoreError, WaitRecord};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

/// Holds dependencies needed by every handler.
pub struct Daemon {
    /// Single-project store used in tests / legacy code paths.
    pub store: Arc<dyn Store>,
    pub notifier: Notifier,
    /// Multi-project mode: lazily-created per-project stores.
    /// `None` means single-project mode (used by unit tests).
    multi: Option<MultiProject>,
}

struct MultiProject {
    home: PathBuf,
    stores: Arc<Mutex<HashMap<ProjectId, Arc<dyn Store>>>>,
    notifiers: Arc<Mutex<HashMap<ProjectId, Notifier>>>,
}

impl Daemon {
    /// Single-project constructor used by unit tests.
    pub fn new(store: Arc<dyn Store>, notifier: Notifier) -> Self {
        Self { store, notifier, multi: None }
    }

    /// Multi-project constructor used by the real daemon process.
    pub fn new_multi(home: PathBuf) -> Self {
        // Provide a no-op in-memory default so `store`/`notifier` are always valid.
        let default_store: Arc<dyn Store> = Arc::new(convoy_store::MemoryStore::new());
        let default_notifier = Notifier::new(default_store.clone());
        Self {
            store: default_store,
            notifier: default_notifier,
            multi: Some(MultiProject {
                home,
                stores: Arc::new(Mutex::new(HashMap::new())),
                notifiers: Arc::new(Mutex::new(HashMap::new())),
            }),
        }
    }

    /// Resolve (or lazily create) the store and notifier for a project.
    async fn project_store(
        &self,
        project_id: &ProjectId,
    ) -> anyhow::Result<(Arc<dyn Store>, Notifier)> {
        if let Some(mp) = &self.multi {
            let mut stores = mp.stores.lock().await;
            if let Some(store) = stores.get(project_id) {
                let notifiers = mp.notifiers.lock().await;
                let notifier = notifiers.get(project_id).cloned().expect("notifier always set with store");
                return Ok((store.clone(), notifier));
            }
            // Create the per-project DB.
            let project_dir = mp.home.join(".convoy").join("projects").join(project_id.as_str());
            std::fs::create_dir_all(&project_dir)?;
            let db_path = project_dir.join("state.db");
            let store: Arc<dyn Store> = Arc::new(
                SqliteStore::open(&db_path)
                    .map_err(|e| anyhow::anyhow!("open project db: {e}"))?,
            );
            let notifier = Notifier::new(store.clone());
            stores.insert(project_id.clone(), store.clone());
            mp.notifiers.lock().await.insert(project_id.clone(), notifier.clone());
            Ok((store, notifier))
        } else {
            // Single-project mode: use the default store.
            Ok((self.store.clone(), self.notifier.clone()))
        }
    }

    /// Return a snapshot of all currently-loaded (project_id, store) pairs.
    pub async fn all_stores(&self) -> Vec<(ProjectId, Arc<dyn Store>)> {
        if let Some(mp) = &self.multi {
            let stores = mp.stores.lock().await;
            stores.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            vec![]
        }
    }

    /// Return a snapshot of all currently-loaded (project_id, store, notifier) triples.
    pub async fn all_stores_with_notifiers(&self) -> Vec<(ProjectId, Arc<dyn Store>, Notifier)> {
        if let Some(mp) = &self.multi {
            let stores = mp.stores.lock().await;
            let notifiers = mp.notifiers.lock().await;
            stores
                .iter()
                .filter_map(|(k, v)| {
                    notifiers.get(k).map(|n| (k.clone(), v.clone(), n.clone()))
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Bind a UNIX socket and serve forever. Removes existing socket file first.
    pub async fn serve(self: Arc<Self>, socket_path: &Path) -> anyhow::Result<()> {
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }
        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("convoyd listening on {}", socket_path.display());
        loop {
            let (stream, _addr) = listener.accept().await?;
            let me = self.clone();
            tokio::spawn(async move {
                if let Err(e) = me.handle(stream).await {
                    tracing::warn!("client error: {e:?}");
                }
            });
        }
    }

    async fn handle(self: Arc<Self>, mut stream: UnixStream) -> anyhow::Result<()> {
        let (rx, mut tx) = stream.split();
        let mut reader = BufReader::new(rx);
        let mut line = String::new();
        while reader.read_line(&mut line).await? > 0 {
            // Try to parse as Envelope first; fall back to bare Request for
            // backward-compat with unit tests that write raw Request JSON.
            let resp = match serde_json::from_str::<Envelope>(line.trim()) {
                Ok(env) => self.dispatch_envelope(env).await,
                Err(_) => {
                    match serde_json::from_str::<Request>(line.trim()) {
                        Ok(req) => {
                            let (store, notifier) = (self.store.clone(), self.notifier.clone());
                            dispatch_with(req, store, notifier).await
                        }
                        Err(e) => Response::Error { message: format!("bad request: {e}") },
                    }
                }
            };
            let s = serde_json::to_string(&resp)? + "\n";
            tx.write_all(s.as_bytes()).await?;
            line.clear();
        }
        Ok(())
    }

    /// Dispatch an envelope: resolve per-project store, run the op.
    pub async fn dispatch_envelope(&self, env: Envelope) -> Response {
        match self.project_store(&env.project_id).await {
            Ok((store, notifier)) => dispatch_with(env.op, store, notifier).await,
            Err(e) => Response::Error { message: format!("project store error: {e}") },
        }
    }

    /// Dispatch a bare request using the default (single-project) store.
    /// Kept for backward compatibility with unit tests that bypass Envelope.
    #[allow(dead_code)]
    async fn dispatch(&self, req: Request) -> Response {
        dispatch_with(req, self.store.clone(), self.notifier.clone()).await
    }
}

async fn dispatch_with(req: Request, store: Arc<dyn Store>, notifier: Notifier) -> Response {
    match req {
        Request::Ping => Response::Pong,

            Request::RegisterSession { id, agent_tag, pid, nickname, branch, worktree_path } => {
                let nick = match Nickname::new(&nickname) {
                    Ok(n) => n,
                    Err(e) => return Response::Error { message: e.to_string() },
                };
                let agent = convoy_core::Agent::from_tag(&agent_tag)
                    .unwrap_or(convoy_core::Agent::ClaudeCode);
                let args = RegisterArgs {
                    id,
                    agent,
                    pid,
                    nickname: nick,
                    branch,
                    worktree_path,
                };
                match store.register_session(args, chrono::Utc::now()).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Heartbeat { id } => {
                let now = chrono::Utc::now();
                let _ = store.touch_heartbeat(&id, now).await;
                let _ = store.touch_alive(&id, now).await;
                Response::Ok
            }

            Request::EndSession { id } => {
                let now = chrono::Utc::now();
                if let Err(e) = store.release_locks_of(&id).await {
                    tracing::warn!(session=%id, "release_locks_of failed on EndSession: {e:?}");
                }
                match store.end_session(&id, now).await {
                    Ok(()) => {
                        notifier.session_ended(&id).await;
                        Response::Ok
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::SetBranch { id, branch } => {
                match store.set_branch(&id, branch).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Rename { id, new } => {
                let nick = match Nickname::new(&new) {
                    Ok(n) => n,
                    Err(e) => return Response::Error { message: e.to_string() },
                };
                match store.rename_session(&id, nick).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListSessions { include_ended } => {
                let sessions = if include_ended {
                    store.list_all_sessions().await
                } else {
                    store.list_active_sessions().await
                };
                match sessions {
                    Ok(v) => {
                        let values: Vec<serde_json::Value> = v
                            .into_iter()
                            .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
                            .collect();
                        Response::Sessions { sessions: values }
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::UpdateStatus { id, summary } => {
                match store.push_status(&id, summary, chrono::Utc::now()).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::SendMessage { from, to, kind, in_reply_to, body } => {
                let msg_id = MessageId::new();
                let in_reply_to_id = in_reply_to.map(MessageId::from_string_unchecked);
                let msg = Message {
                    id: msg_id.clone(),
                    from: from.clone(),
                    to: to.clone(),
                    kind,
                    in_reply_to: in_reply_to_id,
                    body,
                    created_at: chrono::Utc::now(),
                    read_at: None,
                };
                match store.insert_message(msg).await {
                    Ok(()) => {
                        notifier.message_created(&from, to.as_ref(), kind).await;
                        Response::MessageCreated { id: msg_id.to_string() }
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ReadInbox { id, unread_only, limit } => {
                match store.inbox(&id, unread_only, limit).await {
                    Ok(msgs) => {
                        let values: Vec<serde_json::Value> = msgs
                            .into_iter()
                            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
                            .collect();
                        Response::Inbox { messages: values }
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::MarkRead { ids } => {
                let msg_ids: Vec<MessageId> = ids
                    .into_iter()
                    .map(MessageId::from_string_unchecked)
                    .collect();
                match store.mark_read(&msg_ids, chrono::Utc::now()).await {
                    Ok(marked) => Response::MarkRead { marked },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ClaimFile { session, abs_path, reason, ttl_sec } => {
                let now = chrono::Utc::now();
                let expires_at = now + chrono::Duration::seconds(ttl_sec);
                let lock = FileLock {
                    abs_path: abs_path.clone(),
                    session_id: session.clone(),
                    reason: reason.clone(),
                    claimed_at: now,
                    expires_at,
                };
                match store.claim_file(lock).await {
                    Ok(()) => {
                        // Emit a ClaimNotice broadcast message
                        let notice_id = MessageId::new();
                        let notice = Message {
                            id: notice_id,
                            from: session.clone(),
                            to: None,
                            kind: convoy_core::MessageKind::ClaimNotice,
                            in_reply_to: None,
                            body: format!("claimed {:?}", abs_path),
                            created_at: chrono::Utc::now(),
                            read_at: None,
                        };
                        let _ = store.insert_message(notice).await;
                        notifier
                            .message_created(&session, None, convoy_core::MessageKind::ClaimNotice)
                            .await;
                        Response::ClaimResult {
                            claimed: true,
                            held_by: None,
                            held_until: None,
                            expires_at: Some(expires_at.timestamp()),
                        }
                    }
                    Err(StoreError::LockHeld(holder)) => {
                        // Look up when the existing lock expires
                        let held_until = store
                            .lock_for(&abs_path)
                            .await
                            .ok()
                            .flatten()
                            .map(|l| l.expires_at.timestamp());
                        Response::ClaimResult {
                            claimed: false,
                            held_by: Some(holder),
                            held_until,
                            expires_at: None,
                        }
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ReleaseFile { session, abs_path } => {
                // Capture lock before releasing for notifier
                let lock_before = store.lock_for(&abs_path).await.ok().flatten();
                match store.release_file(&abs_path, &session).await {
                    Ok(()) => {
                        if let Some(lock) = lock_before {
                            notifier.lock_released(&lock).await;
                        }
                        Response::Ok
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListLocks => {
                match store.list_locks().await {
                    Ok(locks) => Response::Locks { locks },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListMyClaims { session } => {
                match store.locks_held_by(&session).await {
                    Ok(locks) => Response::Locks { locks },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::WaitFor { session, condition, timeout_sec, hint } => {
                // Check if condition is already satisfied
                let already_satisfied = match &condition {
                    convoy_core::WaitCondition::LockReleased { abs_path } => {
                        // Satisfied if no lock or lock is expired
                        match store.lock_for(abs_path).await {
                            Ok(None) => true,
                            Ok(Some(lock)) => lock.is_expired(chrono::Utc::now()),
                            Err(_) => false,
                        }
                    }
                    convoy_core::WaitCondition::SessionEnded { session_id } => {
                        match store.get_session(session_id).await {
                            Ok(s) => s.ended_at.is_some(),
                            Err(StoreError::NotFound) => true, // session gone = ended
                            Err(_) => false,
                        }
                    }
                    convoy_core::WaitCondition::MessageReceived { .. } => false,
                };

                if already_satisfied {
                    return Response::WaitCreated {
                        wait_id: String::new(),
                        status: "already-satisfied".into(),
                    };
                }

                let now = chrono::Utc::now();
                let wait_id = uuid::Uuid::new_v4().to_string();
                let record = WaitRecord {
                    id: wait_id.clone(),
                    session_id: session,
                    condition,
                    hint,
                    created_at: now,
                    expires_at: now + chrono::Duration::seconds(timeout_sec),
                    satisfied_at: None,
                    outcome: None,
                };
                match store.create_wait(record).await {
                    Ok(()) => Response::WaitCreated {
                        wait_id,
                        status: "waiting".into(),
                    },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::CancelWait { wait_id } => {
                match store.cancel_wait(&wait_id).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Shutdown => Response::Ok,
        }
}
