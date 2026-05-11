pub async fn run(keep_days: i64) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap();
    let projects_dir = home.join(".convoy/projects");
    if !projects_dir.exists() {
        println!("no projects to gc");
        return Ok(());
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(keep_days);
    let cutoff_ts = cutoff.timestamp();

    for entry in std::fs::read_dir(&projects_dir)? {
        let entry = entry?;
        let db_path = entry.path().join("state.db");
        if !db_path.exists() { continue; }

        // Open SQLite directly and run the DELETE
        let conn = rusqlite::Connection::open(&db_path)?;
        let deleted = conn.execute(
            "DELETE FROM sessions WHERE ended_at IS NOT NULL AND ended_at < ?1",
            rusqlite::params![cutoff_ts],
        )?;
        if deleted > 0 {
            println!("gc {:?}: deleted {} ended sessions", entry.path(), deleted);
        }
    }
    Ok(())
}
