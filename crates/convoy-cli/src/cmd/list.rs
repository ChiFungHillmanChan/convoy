use std::path::PathBuf;

pub fn run() -> anyhow::Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no HOME"))?;
    let registry = home.join(".convoy/projects.toml");
    if !registry.exists() {
        println!("(no projects registered)");
        return Ok(());
    }
    let text = std::fs::read_to_string(&registry)?;
    println!("{text}");
    Ok(())
}
