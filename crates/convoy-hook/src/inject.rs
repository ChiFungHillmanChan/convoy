//! Writes `<convoy-update>` blocks to stdout for Claude Code to inject.

use std::io::Write;

pub fn emit(content: &str) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut h = stdout.lock();
    writeln!(
        h,
        "<convoy-update timestamp=\"{}\">\n{}\n</convoy-update>",
        chrono::Utc::now().to_rfc3339(),
        content
    )
}
