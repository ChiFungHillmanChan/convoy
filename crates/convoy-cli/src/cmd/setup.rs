use serde_json::{Map, Value};

pub fn run(yes: bool) -> anyhow::Result<()> {
    let home = dirs::home_dir().unwrap();
    let settings = home.join(".claude/settings.json");
    let body = if settings.exists() {
        std::fs::read_to_string(&settings)?
    } else {
        "{}".into()
    };
    let mut root: Value = serde_json::from_str(&body)?;
    let hooks_obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json is not an object"))?
        .entry("hooks").or_insert_with(|| Value::Object(Map::new()));

    let want = serde_json::json!({
        "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": "convoy hook session-start" }] }],
        "PreToolUse":   [{ "matcher": "", "hooks": [{ "type": "command", "command": "convoy hook pre-tool-use" }] }],
        "PostToolUse":  [{ "matcher": "", "hooks": [{ "type": "command", "command": "convoy hook post-tool-use" }] }],
        "UserPromptSubmit": [{ "matcher": "", "hooks": [{ "type": "command", "command": "convoy hook user-prompt-submit" }] }],
        "Stop":         [{ "matcher": "", "hooks": [{ "type": "command", "command": "convoy hook stop" }] }],
    });

    // Diff preview
    println!("Proposed hook additions:");
    println!("{}", serde_json::to_string_pretty(&want)?);

    if !yes {
        println!("Re-run with --yes to apply.");
        return Ok(());
    }

    *hooks_obj = want;
    std::fs::write(&settings, serde_json::to_string_pretty(&root)?)?;
    println!("wrote hooks to {}", settings.display());
    println!();
    println!("Hooks installed. Use 'convoy session ...' subcommands directly via Bash.");
    Ok(())
}
