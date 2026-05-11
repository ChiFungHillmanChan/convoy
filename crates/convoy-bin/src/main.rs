//! Top-level `convoy` binary — dispatches to sub-crate entry points.

use clap::{Parser, Subcommand};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

/// Convoy: coordination layer for AI coding agents.
#[derive(Parser, Debug)]
#[command(name = "convoy", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the coordination daemon (heartbeat probe + expiry + export loops).
    Daemon {
        /// Run in the foreground (do not daemonize).
        #[arg(long)]
        foreground: bool,
    },
    /// List projects.
    List,
    /// Show session/lock/mail status for a project.
    Status {
        /// Project path (default: cwd).
        project: Option<PathBuf>,
    },
    /// Show a markdown export section.
    Show {
        project: PathBuf,
        #[arg(long, default_value = "sessions")]
        section: String,
    },
    /// Export project state to a JSON bundle.
    Export {
        project: PathBuf,
        #[arg(long, default_value = "convoy-export")]
        to: PathBuf,
        #[arg(long)]
        redacted: bool,
    },
    /// Mark a project finished.
    Finish {
        project: PathBuf,
    },
    /// Reopen a finished project.
    Reopen {
        project: PathBuf,
    },
    /// Delete all convoy state for a project. Requires --yes.
    Forget {
        project: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Check daemon health, schema versions, and lock sanity.
    Doctor,
    /// Delete old ended sessions.
    Gc {
        #[arg(long, default_value = "30")]
        older_than: i64,
    },
    /// Run a Claude Code lifecycle hook (invoked by hooks, not by users).
    Hook {
        /// Hook event name: session-start, user-prompt-submit, pre-tool-use,
        /// post-tool-use, stop
        event: String,
    },
    /// Interactive setup: writes hooks into ~/.claude/settings.json.
    Setup {
        #[arg(long)]
        yes: bool,
    },
    /// Session coordination subcommands (for direct Bash use).
    Session(convoy_cli::cmd::session::SessionArgs),
    /// MCP integration (deferred to M1.1).
    Mcp,
    /// Print version.
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Daemon { foreground } => run_daemon(foreground).await?,

        Cmd::List => convoy_cli::cmd::list::run()?,

        Cmd::Status { project } => {
            convoy_cli::cmd::status::run(project.as_deref()).await?
        }

        Cmd::Show { project, section } => {
            convoy_cli::cmd::show::run(&project, &section)?
        }

        Cmd::Export { project, to, redacted } => {
            convoy_cli::cmd::export::run(&project, &to, redacted)?
        }

        Cmd::Finish { project } => {
            convoy_cli::cmd::finish::run(&project)?
        }

        Cmd::Reopen { project } => {
            convoy_cli::cmd::finish::run_reopen(&project)?
        }

        Cmd::Forget { project, yes } => {
            convoy_cli::cmd::forget::run(&project, yes)?
        }

        Cmd::Doctor => convoy_cli::cmd::doctor::run().await?,

        Cmd::Gc { older_than } => {
            convoy_cli::cmd::gc::run(older_than).await?
        }

        Cmd::Hook { event } => run_hook(&event).await?,

        Cmd::Setup { yes } => convoy_cli::cmd::setup::run(yes)?,

        Cmd::Session(args) => convoy_cli::cmd::session::run(args).await?,

        Cmd::Mcp => {
            println!("convoy: native MCP integration is deferred to M1.1.");
            println!("Use `convoy session ...` subcommands directly via Bash for now.");
            std::process::exit(0);
        }

        Cmd::Version => {
            println!("convoy {}", env!("CARGO_PKG_VERSION"));
        }
    }
    Ok(())
}

async fn run_hook(event: &str) -> anyhow::Result<()> {
    match event {
        "session-start" => convoy_hook::events::session_start::run().await?,
        "user-prompt-submit" => convoy_hook::events::user_prompt_submit::run().await?,
        "pre-tool-use" => convoy_hook::events::pre_tool_use::run().await?,
        "post-tool-use" => convoy_hook::events::post_tool_use::run().await?,
        "stop" => convoy_hook::events::stop::run().await?,
        other => {
            eprintln!("convoy hook: unknown event '{other}'");
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_daemon(foreground: bool) -> anyhow::Result<()> {
    use convoy_daemon::{
        expiry, exports, liveness,
        notify::Notifier,
        server::Daemon,
    };
    use convoy_store::SqliteStore;
    use std::time::Duration;

    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no HOME dir"))?;
    let convoy_dir = home.join(".convoy");
    std::fs::create_dir_all(&convoy_dir)?;

    // Per-project state lives under ~/.convoy/projects/<pid>/
    // For the daemon's own DB we use a shared "daemon" project.
    let daemon_db_dir = convoy_dir.join("daemon");
    std::fs::create_dir_all(&daemon_db_dir)?;
    let db_path = daemon_db_dir.join("state.db");
    let store: Arc<dyn convoy_store::Store> = Arc::new(SqliteStore::open(&db_path)?);

    let notifier = Notifier::new(store.clone());
    let socket_path = convoy_dir.join("daemon.sock");

    // Write PID file.
    let pid_path = convoy_dir.join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Daemonize if not foreground (best-effort: noop on unsupported platforms).
    if !foreground {
        // We keep it simple: spawn a detached child and exit the parent.
        // If --foreground is omitted we re-exec with --foreground from child.
        let exe = std::env::current_exe()?;
        std::process::Command::new(exe)
            .arg("daemon")
            .arg("--foreground")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        return Ok(());
    }

    tracing::info!("convoyd starting (pid {})", std::process::id());

    let daemon = Arc::new(Daemon::new(store.clone(), notifier.clone()));

    // Spawn background loops.
    let store_liveness = store.clone();
    tokio::spawn(async move {
        liveness::run(
            store_liveness,
            Duration::from_secs(60),
            chrono::Duration::seconds(300),
        )
        .await;
    });

    let store_expiry = store.clone();
    let notifier_expiry = notifier.clone();
    tokio::spawn(async move {
        expiry::run(store_expiry, Duration::from_secs(30), notifier_expiry).await;
    });

    let store_exports = store.clone();
    let exports_dir = daemon_db_dir.clone();
    tokio::spawn(async move {
        exports::run(store_exports, exports_dir, Duration::from_secs(10)).await;
    });

    // Serve. chmod the socket to 0600 shortly after binding.
    let sock_for_chmod = socket_path.clone();
    tokio::spawn(async move {
        chmod_socket(&sock_for_chmod).await;
    });

    daemon.serve(&socket_path).await?;
    Ok(())
}

/// Polls for the socket file to appear (up to 1s) then chmod 600.
async fn chmod_socket(path: &std::path::Path) {
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if path.exists() {
            let perms = std::fs::Permissions::from_mode(0o600);
            if let Err(e) = tokio::fs::set_permissions(path, perms).await {
                tracing::warn!("chmod daemon.sock: {e}");
            }
            return;
        }
    }
    tracing::warn!("chmod_socket: socket never appeared at {}", path.display());
}
