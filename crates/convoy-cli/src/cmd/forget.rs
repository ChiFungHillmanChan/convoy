use std::path::Path;

pub fn run(project_path: &Path, confirm: bool) -> anyhow::Result<()> {
    if !confirm { anyhow::bail!("refusing to forget without --yes"); }
    let pid = convoy_core::ProjectId::from_canonical_path(project_path);
    let dir = dirs::home_dir().unwrap().join(format!(".convoy/projects/{}", pid));
    std::fs::remove_dir_all(&dir)?;
    println!("forgot {}", project_path.display());
    Ok(())
}
