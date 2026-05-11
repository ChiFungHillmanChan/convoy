use std::path::Path;

pub fn run(project_path: &Path) -> anyhow::Result<()> {
    let pid = convoy_core::ProjectId::from_canonical_path(project_path);
    let meta_path = dirs::home_dir().unwrap().join(format!(".convoy/projects/{}/meta.toml", pid));
    let mut text = std::fs::read_to_string(&meta_path)?;
    text = text.replace("status = \"active\"", "status = \"finished\"");
    std::fs::write(meta_path, text)?;
    println!("finished {}", project_path.display());
    Ok(())
}

pub fn run_reopen(project_path: &Path) -> anyhow::Result<()> {
    let pid = convoy_core::ProjectId::from_canonical_path(project_path);
    let meta_path = dirs::home_dir().unwrap().join(format!(".convoy/projects/{}/meta.toml", pid));
    let mut text = std::fs::read_to_string(&meta_path)?;
    text = text.replace("status = \"finished\"", "status = \"active\"");
    std::fs::write(meta_path, text)?;
    println!("reopened {}", project_path.display());
    Ok(())
}
