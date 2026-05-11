use convoy_store::{SqliteStore, Store};
use std::path::Path;

pub async fn run(project_path: Option<&Path>) -> anyhow::Result<()> {
    let project_path = project_path.map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let pid = convoy_core::ProjectId::from_canonical_path(&project_path);
    let home = dirs::home_dir().unwrap();
    let db = home.join(format!(".convoy/projects/{}/state.db", pid));
    if !db.exists() {
        println!("no convoy state for {}", project_path.display());
        return Ok(());
    }
    let store = SqliteStore::open(&db)?;
    for s in store.list_active_sessions().await? {
        let st = store.latest_status(&s.id).await?.unwrap_or_default();
        println!("- {} (#{}) [{}]  {}", s.nickname, s.id.short(), s.pid, st);
    }
    Ok(())
}
