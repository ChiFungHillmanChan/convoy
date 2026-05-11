# Convoy

Local coordination layer for AI coding agents. Two Claude Code sessions on the
same project (or different worktrees) can discover each other, share status,
send messages, claim files, and wait for peers — so they do not stomp on each
other's work.

**Status:** M1 in development. See `docs/specs/2026-05-11-m1-design.md` and
`docs/plans/2026-05-11-m1-implementation.md`.

## Quick start (once M1 lands)

```bash
cargo install --path crates/convoy-bin
convoy setup
```

`convoy setup` writes the required Claude Code hooks and prints the
`claude mcp add` command.

## License

MIT.
