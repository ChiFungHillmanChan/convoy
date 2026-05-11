use std::path::Path;

pub fn run(project_path: &Path, section: &str) -> anyhow::Result<()> {
    let pid = convoy_core::ProjectId::from_canonical_path(project_path);
    let home = dirs::home_dir().unwrap();
    let p = home.join(format!(".convoy/projects/{}/exports/{}.md", pid, section));
    let body = std::fs::read_to_string(&p)?;
    print!("{body}");
    Ok(())
}
