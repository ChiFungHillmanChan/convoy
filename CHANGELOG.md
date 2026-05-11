# Changelog

All notable changes to Convoy are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned for v0.2.0 (M1.1)
- Native MCP tool surface (`convoy mcp` stdio server) once `rmcp` stabilises
- Linux CI coverage for `liveness::pid_alive` (currently macOS-tested only)

### Planned for v0.3.0 (M2)
- Codex CLI adapter via Bash shim that wraps `convoy session ...`
- Gemini CLI adapter via Bash shim
- `agent` column populated with `codex` / `gemini` values

---

## [0.1.0] — 2026-05-11

First public release. Single-host coordination layer for Claude Code sessions.

### Added

**Core (`convoy-core`)** — pure domain types, no I/O:
- `SessionId`, `Nickname` (validated), `ProjectId` (sha256-derived)
- `FileLock` with TTL expiry
- `Message`, `MessageKind` (info / question / ack / request-yield / claim-notice / status-broadcast)
- `WaitCondition` + event matcher (lock-released / session-ended / message-received)
- `RateLimiter` (rolling window)
- `Session` aggregate + `Agent` + `LivenessStatus`

**Storage (`convoy-store`)** — Store trait with two impls:
- `MemoryStore` for tests
- `SqliteStore` for production (WAL, busy_timeout=5000ms, lazy migration)
- Cross-store parity test harness (every method runs against both)
- Proptest invariant: every unexpired lock has exactly one holder (runs
  against both stores)
- Atomic `claim_file` via `BEGIN IMMEDIATE` + `INSERT OR IGNORE`

**Daemon (`convoy-daemon`)** — multi-project coordination process:
- UNIX socket RPC at `~/.convoy/daemon.sock`
- `Envelope { project_id, op }` wire protocol; daemon holds
  `HashMap<ProjectId, Arc<dyn Store>>` populated lazily
- Liveness probe loop (`kill(pid, 0)` every 60s, reaps after 5min silence)
- Expiry sweep loop (releases TTL-expired locks; times out expired waits)
- Notification dispatcher (`Notifier` wakes pending waits on lock-released /
  session-ended / message-created events)
- Exports writer (regenerates per-project `sessions.md`, `recent_mail.md`,
  `locks.md` every 10s)
- `DaemonClient` for in-process callers (hooks, CLI)

**Hooks (`convoy-hook`)** — five Claude Code lifecycle hooks:
- `session-start` — auto-register, nickname from branch name (fallback random
  adjective-animal), inject peer summary into context
- `user-prompt-submit` — touch heartbeat, inject unread mail + satisfied waits
- `pre-tool-use` — heartbeat; advisory warning when Write/Edit/NotebookEdit
  targets a file held by another session
- `post-tool-use` — heartbeat; if last tool was `git checkout|switch|worktree`,
  refresh tracked branch (regex cached via `OnceLock`)
- `stop` — graceful deregister, release held locks, broadcast session-ended

**CLI (`convoy-cli`)** — two command groups:
- User-facing: `list`, `status`, `show`, `export`, `finish`, `reopen`,
  `forget`, `doctor`, `gc`, `setup`
- `session` subcommand group (Bash interface for the Claude model):
  `send`, `claim`, `release`, `inbox`, `mark-read`, `status`, `list-sessions`,
  `locks`, `wait`, `cancel-wait`, `rename`
- All `session` subcommands read `CLAUDE_SESSION_ID` env (with `--session
  <id>` override for tests / non-Claude callers)

**Binary (`convoy-bin`)** — top-level `convoy` with subcommands:
- `convoy daemon [--foreground]` — wire stores + loops + serve socket
- `convoy hook <event>` — dispatch to one of the 5 hook event handlers
- `convoy session ...` — delegate to CLI session group
- `convoy mcp` — placeholder, deferred to M1.1

**Setup**:
- `convoy setup --yes` writes hooks into `~/.claude/settings.json` with
  a preview-then-apply flow

**Acceptance tests (`convoy-e2e`)** — 10 scenarios from spec §15:
- Two-session discovery, lock conflict, status visibility, send/receive,
  wait satisfied by release, SIGKILL recovery, `convoy status` output,
  `finish` + `forget` workflow, `doctor` clean run, CI suite passing

### Storage layout

```
~/.convoy/
+-- daemon.sock                  (transient, UNIX socket)
+-- daemon.pid                   (transient, single-instance guard)
+-- projects/
    +-- <project-id>/            (project_id = sha256(realpath(git_common_dir or cwd))[:12])
        +-- meta.toml            (path, created_at, status)
        +-- state.db             (SQLite WAL)
        +-- exports/             (sessions.md, recent_mail.md, locks.md)
        +-- archive/             (snapshots from `convoy finish`)
```

Git worktrees of the same repo always map to the same `project_id`.

### Notable design choices

- **Hooks for awareness, Bash CLI for active operations.** Native MCP
  deferred until `rmcp` SDK stabilises. The Bash CLI is what M2 will reuse
  for Codex / Gemini anyway, so it is not throwaway work.
- **Cooperative `wait_for`** — non-blocking, satisfaction delivered on the
  next hook injection. Reflects the reality that LLM turns can't actually
  block.
- **Rate-limited inter-agent traffic** — 20 messages/min, 5 broadcasts/min,
  10 claims/min per session. Defaults configurable in `~/.convoy/config.toml`.
- **Per-project structural isolation** — `rm -rf
  ~/.convoy/projects/<id>/` fully forgets one project without affecting
  others.

### Known limitations (not blockers, tracked for follow-ups)

- `convoy doctor` does not yet auto-fix any issues; it only reports
- `random_animal` nickname fallback has skewed distribution
- `pending_waits()` is a full table scan per notification event (fine at
  M1 scale; future optimisation for many concurrent waits)
- `crash_recovery` e2e test uses a hardcoded `/tmp/` path (parallel test
  runs on the same machine could conflict)
- `libc::__error()` in `liveness.rs` is macOS-only; Linux CI confirms it
  compiles on `ubuntu-latest`, but a future port may need
  `std::io::Error::last_os_error().raw_os_error()`

### Tests

- 82 tests total: 34 unit (core), 13 unit + 13 integration + 1 proptest (store),
  2 unit + 4 integration (daemon), 2 unit (cli), 13 unit (hook), 9 e2e

[Unreleased]: https://github.com/ChiFungHillmanChan/convoy/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ChiFungHillmanChan/convoy/releases/tag/v0.1.0
