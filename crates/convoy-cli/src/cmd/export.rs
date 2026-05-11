use convoy_store::{SqliteStore, Store};
use std::path::Path;

pub fn run(project_path: &Path, to: &Path, _redacted: bool) -> anyhow::Result<()> {
    let pid = convoy_core::ProjectId::from_canonical_path(project_path);
    let home = dirs::home_dir().unwrap();
    let db = home.join(format!(".convoy/projects/{}/state.db", pid));
    if !db.exists() {
        anyhow::bail!("no convoy state for {}", project_path.display());
    }

    std::fs::create_dir_all(to)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let store = SqliteStore::open(&db)?;

        let sessions = store.list_all_sessions().await?;
        let sessions_json = serde_json::to_string_pretty(&sessions)?;
        std::fs::write(to.join("sessions.json"), sessions_json)?;

        let messages = store.recent_messages(usize::MAX).await?;
        let messages_json = serde_json::to_string_pretty(&messages)?;
        std::fs::write(to.join("messages.json"), messages_json)?;

        let locks = store.list_locks().await?;
        let locks_json = serde_json::to_string_pretty(&locks)?;
        std::fs::write(to.join("locks.json"), locks_json)?;

        println!("exported to {}", to.display());
        Ok::<(), anyhow::Error>(())
    })
}
