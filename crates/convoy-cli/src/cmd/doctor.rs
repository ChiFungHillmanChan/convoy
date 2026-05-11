#[allow(unsafe_code)]
pub async fn run() -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap();
    let pid_file = home.join(".convoy/daemon.pid");
    if pid_file.exists() {
        let pid: u32 = std::fs::read_to_string(&pid_file)?.trim().parse()?;
        // SAFETY: kill(pid, 0) is a read-only probe — no signal is delivered.
        let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
        if !alive { println!("warn: stale daemon.pid (pid {pid} dead)"); }
        else { println!("ok: daemon running (pid {pid})"); }
    } else {
        println!("info: no daemon.pid (daemon not running)");
    }

    // Walk projects/ and check each state.db's schema_version
    let projects_dir = home.join(".convoy/projects");
    if projects_dir.exists() {
        for entry in std::fs::read_dir(&projects_dir)? {
            let entry = entry?;
            let db = entry.path().join("state.db");
            if db.exists() {
                match convoy_store::SqliteStore::open(&db) {
                    Ok(_) => println!("ok: {:?}", entry.path()),
                    Err(e) => println!("err: {:?}: {e}", entry.path()),
                }
            }
        }
    } else {
        println!("info: no projects directory at {}", projects_dir.display());
    }
    Ok(())
}
