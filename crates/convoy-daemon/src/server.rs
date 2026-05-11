//! Daemon RPC server: accepts on a UNIX socket, dispatches to handlers.

use crate::notify::Notifier;
use crate::rpc::{Request, Response};
use convoy_core::{FileLock, Message, MessageId, Nickname};
use convoy_store::{RegisterArgs, Store, StoreError, WaitRecord};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

/// Holds dependencies needed by every handler.
pub struct Daemon {
    pub store: Arc<dyn Store>,
    pub notifier: Notifier,
}

impl Daemon {
    pub fn new(store: Arc<dyn Store>, notifier: Notifier) -> Self {
        Self { store, notifier }
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
            let req: Request = match serde_json::from_str(line.trim()) {
                Ok(r) => r,
                Err(e) => {
                    let resp = Response::Error { message: format!("bad request: {e}") };
                    let s = serde_json::to_string(&resp)? + "\n";
                    tx.write_all(s.as_bytes()).await?;
                    line.clear();
                    continue;
                }
            };
            let resp = self.dispatch(req).await;
            let s = serde_json::to_string(&resp)? + "\n";
            tx.write_all(s.as_bytes()).await?;
            line.clear();
        }
        Ok(())
    }

    async fn dispatch(&self, req: Request) -> Response {
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
                match self.store.register_session(args, chrono::Utc::now()).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Heartbeat { id } => {
                let now = chrono::Utc::now();
                let _ = self.store.touch_heartbeat(&id, now).await;
                let _ = self.store.touch_alive(&id, now).await;
                Response::Ok
            }

            Request::EndSession { id } => {
                let now = chrono::Utc::now();
                let _ = self.store.release_locks_of(&id).await;
                match self.store.end_session(&id, now).await {
                    Ok(()) => {
                        self.notifier.session_ended(&id).await;
                        Response::Ok
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::SetBranch { id, branch } => {
                match self.store.set_branch(&id, branch).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Rename { id, new } => {
                let nick = match Nickname::new(&new) {
                    Ok(n) => n,
                    Err(e) => return Response::Error { message: e.to_string() },
                };
                match self.store.rename_session(&id, nick).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListSessions { include_ended } => {
                let sessions = if include_ended {
                    self.store.list_all_sessions().await
                } else {
                    self.store.list_active_sessions().await
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
                match self.store.push_status(&id, summary, chrono::Utc::now()).await {
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
                match self.store.insert_message(msg).await {
                    Ok(()) => {
                        self.notifier.message_created(&from, to.as_ref(), kind).await;
                        Response::MessageCreated { id: msg_id.to_string() }
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ReadInbox { id, unread_only, limit } => {
                match self.store.inbox(&id, unread_only, limit).await {
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
                match self.store.mark_read(&msg_ids, chrono::Utc::now()).await {
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
                match self.store.claim_file(lock).await {
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
                        let _ = self.store.insert_message(notice).await;
                        self.notifier
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
                        let held_until = self
                            .store
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
                let lock_before = self.store.lock_for(&abs_path).await.ok().flatten();
                match self.store.release_file(&abs_path, &session).await {
                    Ok(()) => {
                        if let Some(lock) = lock_before {
                            self.notifier.lock_released(&lock).await;
                        }
                        Response::Ok
                    }
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListLocks => {
                match self.store.list_locks().await {
                    Ok(locks) => Response::Locks { locks },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::ListMyClaims { session } => {
                match self.store.locks_held_by(&session).await {
                    Ok(locks) => Response::Locks { locks },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::WaitFor { session, condition, timeout_sec, hint } => {
                // Check if condition is already satisfied
                let already_satisfied = match &condition {
                    convoy_core::WaitCondition::LockReleased { abs_path } => {
                        // Satisfied if no lock or lock is expired
                        match self.store.lock_for(abs_path).await {
                            Ok(None) => true,
                            Ok(Some(lock)) => lock.is_expired(chrono::Utc::now()),
                            Err(_) => false,
                        }
                    }
                    convoy_core::WaitCondition::SessionEnded { session_id } => {
                        match self.store.get_session(session_id).await {
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
                match self.store.create_wait(record).await {
                    Ok(()) => Response::WaitCreated {
                        wait_id,
                        status: "waiting".into(),
                    },
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::CancelWait { wait_id } => {
                match self.store.cancel_wait(&wait_id).await {
                    Ok(()) => Response::Ok,
                    Err(e) => Response::Error { message: e.to_string() },
                }
            }

            Request::Shutdown => Response::Ok,
        }
    }
}
