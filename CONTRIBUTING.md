# Contributing to Convoy

Thanks for considering a contribution. Convoy is small, opinionated, and
shaped by a TDD-heavy design process — those expectations apply to PRs too.

## Quick prerequisites

- Rust 1.85+ (the workspace pins this; `rustup` will install it automatically
  on first build via `rust-toolchain.toml`)
- `git` 2.5+ for worktrees if you want to test the multi-session flows
- macOS or Linux. Windows is not supported in M1.

## Build and test

```bash
git clone https://github.com/ChiFungHillmanChan/convoy
cd convoy
cargo build --workspace
cargo test --workspace          # should report 82+ passing tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The CI workflow at `.github/workflows/ci.yml` runs the same checks on every
push and PR. Get these green locally before submitting.

## How we work

### Test-driven development

For new features and bug fixes:

1. Write a failing test that captures the behaviour you want
2. Run it and confirm it fails for the *right* reason
3. Make the smallest change that turns the test green
4. Refactor if the code is unclear
5. Commit with the test and the fix together

If you're contributing a bug fix that can be expressed as "this case used to
fail", a regression test in the same PR is required.

### Small, focused commits

We prefer many small commits over one big squashed one. Each commit message
should explain *why* the change is being made, not just what. Reference an
issue when relevant.

Example commit messages from this repo:

```
feat(store): atomic claim_file via transaction; extend proptest to SqliteStore
fix(daemon): log release_locks_of errors on EndSession
docs(plan): defer Phase 4 (MCP) to M1.1; expand Phase 6 with session subcommands
```

### One problem per PR

Don't combine an unrelated cleanup with a feature. Open separate PRs. If you
notice something worth fixing while in the area, mention it in the PR
description so we can track it.

### File structure

Each file should have one clear responsibility. Don't grow a file past ~500
lines without splitting it into focused modules. Don't restructure code that
isn't part of your change.

## Reporting bugs

Open an issue using the bug template at `.github/ISSUE_TEMPLATE/bug.md`.

Helpful things to include:

- `convoy version`
- OS + Rust version
- Steps to reproduce
- `convoy doctor` output
- A `convoy export <project> --redacted --to /tmp/bundle` JSON bundle if the
  bug touches stored state (the `--redacted` flag masks absolute paths)

## Proposing features

Open an issue using the feature template at
`.github/ISSUE_TEMPLATE/feature.md`. Larger ideas — new modules, protocol
changes, new agent adapters — should sketch the design before code. The
brainstorming → spec → plan → implement loop in `docs/specs/` and
`docs/plans/` is how M1 was built; you're welcome to follow the same shape.

## Security disclosure

Do not file security issues in public. See [SECURITY.md](SECURITY.md).

## Code style

- `cargo fmt` is authoritative for formatting
- `cargo clippy --all-targets -- -D warnings` must pass
- No `unsafe` code outside the `liveness.rs` PID probe (which has an
  `#[allow(unsafe_code)]` with a comment explaining why)
- Prefer `?` over `unwrap()` in production code paths. `unwrap()` in tests
  is fine.
- Doc comments (`///`) on every public type and function. The
  `convoy-core` crate enforces `#![warn(missing_docs)]`.

## License

By contributing, you agree your contributions are licensed under the MIT
license (see [LICENSE](LICENSE)).
