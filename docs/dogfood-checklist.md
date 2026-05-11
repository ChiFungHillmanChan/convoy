# Dogfood Checklist — two real Claude Code sessions

A 10-minute checklist to confirm Convoy v0.1.0 works end-to-end with real
Claude Code processes. Run this in a project you don't mind seeing minor
state in — Convoy writes to `~/.convoy/` and to `~/.claude/settings.json`.

## Prerequisites

- Convoy installed (`convoy --version` reports `0.1.0`)
- Claude Code CLI installed and `claude` works
- A git repo to test in (anything with `.git/`)

## Setup (one time)

```bash
# 1. Write hooks into your Claude Code settings
convoy setup --yes

# 2. Confirm hooks landed
jq '.hooks' ~/.claude/settings.json
# Expected: 5 entries — SessionStart, PreToolUse, PostToolUse,
# UserPromptSubmit, Stop — each calling `convoy hook <event>`

# 3. Start the daemon (will run in the background)
convoy daemon &
sleep 1
ls ~/.convoy/
# Expected: daemon.pid, daemon.sock
```

If something looks wrong:

```bash
convoy doctor                       # health check
cat ~/.convoy/daemon.log            # tracing output (if daemon was run with RUST_LOG=info)
```

## The actual test

### Step 1 — open two Claude sessions

Open **two terminals**, both in the same git repo (you can also use two
worktrees of the same repo — Convoy treats them as one project).

```bash
# terminal 1
cd path/to/repo
claude

# terminal 2
cd path/to/repo                     # (or a sibling worktree)
claude
```

Each session's `SessionStart` hook auto-registers it. You should see an
injected `<convoy-update>` block in each session's first turn announcing
`active peers: N session(s)`.

### Step 2 — verify discovery from a third terminal

```bash
# terminal 3
convoy session list-sessions --session $(uuidgen | tr A-Z a-z)
```

Expected: JSON with both sessions, each with a nickname like
`crimson-lynx` / `bold-cat`. (If you don't want to provide a `--session`,
the command also reads `CLAUDE_SESSION_ID` env, but a third terminal won't
have it set.)

### Step 3 — provoke a file-claim conflict

In session A's Claude prompt, ask:

> Please claim ./src/main.rs for the next 10 minutes, reason "testing".

It should run something like:

```
Bash("convoy session claim $(pwd)/src/main.rs --reason testing --ttl 600")
```

and report `claimed: true`.

In session B's Claude prompt:

> Please try to claim the same ./src/main.rs.

B should get `claimed: false, held_by: <A's id>, held_until: <timestamp>`.

### Step 4 — send a question between sessions

In session A:

> Send a question to session B (find their id with `convoy session list-sessions`) asking whether they're done with config.yaml.

In session B's next turn, the `UserPromptSubmit` hook should auto-inject
something like:

```
<convoy-update timestamp="...">
unread mail (1):
  [question] crimson-lynx: Are you done with config.yaml?
</convoy-update>
```

Ask session B to reply:

> Reply to that question with an ack: "Just released it."

A's next turn should see the ack injected.

### Step 5 — release and immediately re-claim

In session A:

> Release the claim on ./src/main.rs.

In session B:

> Try to claim ./src/main.rs now.

Should succeed.

### Step 6 — observe from outside

```bash
# Any terminal
ls ~/.convoy/projects/
# One folder, 12 hex chars

PROJ_ID=$(ls ~/.convoy/projects/ | head -1)
cat ~/.convoy/projects/$PROJ_ID/meta.toml
cat ~/.convoy/projects/$PROJ_ID/exports/sessions.md
cat ~/.convoy/projects/$PROJ_ID/exports/recent_mail.md
cat ~/.convoy/projects/$PROJ_ID/exports/locks.md
```

The markdown mirrors regenerate every 10 seconds.

### Step 7 — crash recovery

Close one of the Claude sessions (Ctrl-C / `/exit`). The `Stop` hook should
fire and gracefully deregister.

For a real crash, kill the Claude process from the third terminal:

```bash
pkill -9 -f "claude.*--session" || pkill -9 claude
```

Within ~5 minutes, `convoy session list-sessions` should show the killed
session removed and its locks released. (You can shorten the wait by reading
`crates/convoy-daemon/src/liveness.rs` and patching the threshold for a
test build.)

### Step 8 — cleanup

```bash
convoy finish $(pwd)                 # archive this project's state
convoy forget $(pwd) --yes           # delete this project's folder entirely
kill %1                              # stop the daemon (or use `pkill -f "convoy daemon"`)
```

## What to file as bugs

If any step diverges from the expected outcome:

- Capture `convoy doctor` output
- Capture last ~50 lines of `~/.convoy/daemon.log` if the daemon was launched
  with `RUST_LOG=info`
- Run `convoy export $(pwd) --redacted --to /tmp/convoy-bug.tar`
- Open an issue using the bug template

## Known v0.1.0 limitations to expect

- **`convoy status <path>` may report "no state" for a git project even
  while sessions are active.** The CLI's project_id derivation doesn't yet
  use `git rev-parse --git-common-dir` the way the hook and session
  subcommands do. Workaround: use `convoy session list-sessions` (with
  `--session <some-uuid>`) instead. Tracked for v0.1.1.
- **The `last_status` field is stored but not returned by
  `convoy session list-sessions` JSON.** It IS visible in
  `exports/sessions.md`. Tracked for v0.1.1.
- Daemon must already be running before any Claude session starts (the
  hooks fail-soft if the socket is missing, but auto-registration won't
  happen). Add `convoy daemon &` to your shell's startup if you want it
  always running.
