//! Generates `exports/*.md` from the SQL state.

use convoy_store::Store;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub async fn run(
    store: Arc<dyn Store>,
    project_dir: PathBuf,
    interval: Duration,
) {
    let exports = project_dir.join("exports");
    if let Err(e) = tokio::fs::create_dir_all(&exports).await {
        tracing::warn!("create exports dir: {e}");
    }
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = render_sessions(&store, &exports).await {
            tracing::warn!("render sessions.md: {e}");
        }
        if let Err(e) = render_mail(&store, &exports).await {
            tracing::warn!("render mail: {e}");
        }
        if let Err(e) = render_locks(&store, &exports).await {
            tracing::warn!("render locks: {e}");
        }
    }
}

pub async fn render_sessions(store: &Arc<dyn Store>, exports: &std::path::Path) -> anyhow::Result<()> {
    use std::fmt::Write;
    let sessions = store.list_all_sessions().await?;
    let mut out = String::new();
    writeln!(out, "# Sessions")?;
    writeln!(out)?;
    writeln!(out, "Updated: {}  (auto-generated, do not edit)", chrono::Utc::now().to_rfc3339())?;
    writeln!(out)?;
    let (active, ended): (Vec<_>, Vec<_>) = sessions.into_iter().partition(|s| s.ended_at.is_none());
    writeln!(out, "## Active ({})", active.len())?;
    for s in active {
        writeln!(out, "### {}  (#{}, branch {:?})", s.nickname, s.id.short(), s.branch)?;
        let st = store.latest_status(&s.id).await.ok().flatten().unwrap_or_default();
        writeln!(out, "- pid: {}", s.pid)?;
        writeln!(out, "- started: {}", s.started_at.to_rfc3339())?;
        writeln!(out, "- last status: {st:?}")?;
        writeln!(out)?;
    }
    writeln!(out, "## Ended ({})", ended.len())?;
    for s in ended {
        writeln!(out, "- {} ended {:?}", s.nickname, s.ended_at)?;
    }
    tokio::fs::write(exports.join("sessions.md"), out).await?;
    Ok(())
}

pub async fn render_mail(store: &Arc<dyn Store>, exports: &std::path::Path) -> anyhow::Result<()> {
    use std::fmt::Write;
    let msgs = store.recent_messages(100).await?;
    let mut out = String::from("# Recent mail\n\n");
    for m in msgs {
        writeln!(
            out,
            "- [{}] {} -> {} : {}",
            m.kind.as_tag(),
            m.from.short(),
            m.to.as_ref().map(|t| t.short().to_string()).unwrap_or_else(|| "*".into()),
            m.body.replace('\n', " ")
        )?;
    }
    tokio::fs::write(exports.join("recent_mail.md"), out).await?;
    Ok(())
}

pub async fn render_locks(store: &Arc<dyn Store>, exports: &std::path::Path) -> anyhow::Result<()> {
    use std::fmt::Write;
    let locks = store.list_locks().await?;
    let mut out = String::from("# Active locks\n\n");
    for l in locks {
        writeln!(
            out,
            "- {} held by {} (reason: {}) until {}",
            l.abs_path.display(),
            l.session_id.short(),
            l.reason.as_deref().unwrap_or(""),
            l.expires_at.to_rfc3339(),
        )?;
    }
    tokio::fs::write(exports.join("locks.md"), out).await?;
    Ok(())
}
