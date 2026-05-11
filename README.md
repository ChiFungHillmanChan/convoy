# Convoy

> Local coordination layer for AI coding agents.

Two or more Claude Code sessions running in the same project (or in different
git worktrees of the same repo) can discover each other, share status, send
messages, claim files, and wait for peers — so they don't stomp on each
other's work.

**Status:** v0.1.0 (M1). Single-host, Claude Code only. Codex / Gemini
adapters in M2. Native MCP integration in M1.1.

---

## Why Convoy

If you run multiple AI coding agents on the same machine, sooner or later
they will:

- Edit the same file at the same time and clobber each other
- Run conflicting `git` operations (stash, rebase, checkout) from underneath
  each other
- Repeat the same investigation independently
- Sit idle waiting for a file they could have known was about to be released

Convoy is a thin local layer that gives them just enough shared context to
behave like co-workers instead of strangers.

```
+----------------+        +----------------+
|  Claude  A     |        |  Claude  B     |
|  (terminal 1)  |        |  (terminal 2)  |
|  branch X      |        |  branch Y      |
+-------+--------+        +-------+--------+
        |                         |
        +------ hooks + CLI ------+
                   |
            +------+------+
            |   convoyd   |
            |  (one per   |
            |   host)     |
            +------+------+
                   |
            +------+------+
            |  SQLite per |
            |   project   |
            +-------------+
```

---

## What you get in v0.1.0

- **Discovery** — `convoy session list-sessions` returns every active Claude
  on the same project, with branch + last status.
- **File claims** — `convoy session claim <path>` reserves a file for your
  session. Other sessions trying to claim get `held_by` + `held_until`.
  Claims auto-expire on TTL (default 30 min).
- **Inbox** — peers can send each other typed messages (`info`, `question`,
  `ack`, `request-yield`, `status-broadcast`). Auto-injected into the next
  Claude turn via hooks.
- **Cooperative wait** — `convoy session wait` registers a non-blocking
  subscription on a condition (lock released, peer ended, message received).
  Wake-up is delivered on the next Claude turn.
- **Crash recovery** — if a Claude session is SIGKILL'd, the daemon's PID
  probe reaps it within ~5 minutes and releases its locks.
- **Per-project isolation** — every project has its own SQLite at
  `~/.convoy/projects/<id>/state.db`. Worktrees of the same repo share state
  via `git rev-parse --git-common-dir`. `rm -rf` the folder = full reset for
  that project.
- **Transparent storage** — auto-generated markdown mirror in `exports/`:
  `sessions.md`, `recent_mail.md`, `locks.md`. `cat` it any time without
  touching SQLite.

---

## Install

Requires Rust 1.85+ (workspace pins this in `rust-toolchain.toml`).

```bash
git clone https://github.com/ChiFungHillmanChan/convoy
cd convoy
cargo install --path crates/convoy-bin
```

This puts a `convoy` binary in `~/.cargo/bin/`.

---

## Quick start

```bash
# 1. Write hooks into ~/.claude/settings.json
convoy setup --yes

# 2. Start the coordination daemon
convoy daemon &                    # detaches; logs to ~/.convoy/daemon.log

# 3. Open two Claude Code sessions in two terminals, both inside the
#    same git repo (or two worktrees of the same repo). They auto-register.

# 4. Inspect from a third terminal
convoy list                        # all projects
convoy status                      # this project's sessions + locks + mail
convoy show $(pwd) --section sessions
```

The Claude model in each session can use any of these via `Bash`:

```bash
convoy session list-sessions
convoy session claim ./src/main.rs --reason "refactoring login"
convoy session send --to <peer-id> --kind question "Are you done with config.yaml?"
convoy session inbox --unread-only
convoy session wait --condition '{"type":"lock_released","abs_path":"./x.rs"}'
convoy session status "phase 1 done, starting tests"
convoy session release ./src/main.rs
```

The `SessionStart` and `UserPromptSubmit` hooks auto-inject peer state, unread
messages, and satisfied waits into the model's context — the model doesn't
have to remember to poll.

---

## What's coordinating exactly

Each `~/.convoy/projects/<project-id>/` holds:

```
meta.toml        # project_path, created_at, status (active|finished)
state.db         # SQLite WAL: sessions, messages, file_locks, waits, ...
exports/         # human-readable markdown mirror, regenerated every 10s
archive/         # snapshots written by `convoy finish`
```

Project id = `sha256(realpath(git_common_dir or cwd))[:12]`. Git worktrees of
the same repo always map to the same project id.

---

## Lifecycle commands

```
convoy list                          # show all known projects
convoy status [<path>]               # active sessions, locks, recent mail
convoy show <path> --section <s>     # render one of: sessions, recent_mail, locks
convoy export <path> --to <dir> [--redacted]
                                     # portable JSON+md bundle for sharing / bug reports
convoy finish <path>                 # archive state, refuse new sessions
convoy reopen <path>                 # undo finish
convoy forget <path> --yes           # rm -rf this project's state
convoy doctor                        # check daemon health, schema versions, orphan locks
convoy gc --older-than 30            # drop ended-session rows older than N days
```

---

## Design

Read the design spec at
[`docs/specs/2026-05-11-m1-design.md`](docs/specs/2026-05-11-m1-design.md) and
the TDD implementation plan at
[`docs/plans/2026-05-11-m1-implementation.md`](docs/plans/2026-05-11-m1-implementation.md).

Key choices:

- **Single daemon, per-project SQLite** — one `convoyd` per host, lazy
  `HashMap<ProjectId, Store>` inside it. Crash-recoverable: SQLite is the
  source of truth; daemon owns only liveness + push + exports.
- **Hooks for passive awareness, CLI for active operations** — `SessionStart`
  / `UserPromptSubmit` hooks inject context automatically; the model uses
  `Bash("convoy session ...")` calls for active coordination.
- **Native MCP deferred to M1.1** — `rmcp` is still 0.x; the Bash surface
  works today and is what Codex / Gemini will use in M2 anyway.
- **Cooperative `wait_for`, not blocking** — LLM turns can't actually block;
  waits are subscriptions, satisfaction is delivered on the next hook
  injection.

---

## Roadmap

| Milestone | Scope |
|---|---|
| **v0.1.0 (M1)** | Claude Code only, single host, Bash CLI coordination. **This release.** |
| v0.2.0 (M1.1) | Native MCP tool surface once `rmcp` stabilises |
| v0.3.0 (M2) | Codex CLI + Gemini CLI adapters (already-present `convoy session` Bash interface gets thin shims) |
| v0.4.0 (M3) | Web dashboard at `localhost:7444`; multi-project switcher; comm volume analytics |
| beyond | Cursor / VSCode internal-agent integration; remote host coordination |

---

## Architecture

Seven Rust crates in a Cargo workspace:

```
crates/
+-- convoy-core      # pure domain types, no I/O
+-- convoy-store     # Store trait + MemoryStore + SqliteStore + parity tests
+-- convoy-daemon    # socket server, liveness probe, expiry sweep, notify, exports
+-- convoy-hook      # 5 Claude Code lifecycle hooks
+-- convoy-cli       # user-facing + session-facing subcommands
+-- convoy-bin       # top-level `convoy` binary
+-- convoy-e2e       # workspace integration tests (acceptance scenarios)
```

82 tests cover unit + integration + cross-store parity + proptest race
invariants + 10 e2e acceptance scenarios from the design spec §15.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The short version:

- TDD: write the failing test before the fix
- `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings`
  before pushing
- Each commit should be focused and reviewable independently
- One problem per PR

---

## License

MIT. See [LICENSE](LICENSE).
