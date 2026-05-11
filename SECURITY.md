# Security policy

## Supported versions

Convoy is pre-1.0. Only the latest `0.x` release receives security fixes.

## Reporting a vulnerability

**Do not open a public GitHub issue.** Instead, email
`hillmanchan709@gmail.com` with:

- A description of the vulnerability
- Steps to reproduce
- The version of Convoy affected (`convoy version`)
- The environment (OS, Rust version)

You should expect an acknowledgement within 7 days. If the report is valid,
we'll work on a fix on a private branch and coordinate disclosure with you.

## Scope

Convoy's M1 trust model is single-user local. Specifically:

- The daemon binds only a UNIX socket at `~/.convoy/daemon.sock` (mode 0600).
  No TCP listener.
- All processes running under the same UID are treated as trusted peers.
- State files (`state.db`, `meta.toml`) are written with the user's umask;
  M1 does not enforce 0600 on them. If your home directory permissions are
  permissive, other local users can read coordination state.

Out of scope for M1:

- Per-session authentication (multiple users on one machine)
- Encryption at rest of `state.db`
- Network-reachable daemon (remote host coordination is M3+)
- Sandboxing of hook scripts

If you find a security issue inside the documented scope, please report it
privately as above. Issues outside the documented scope are still welcome
as feature requests on the public issue tracker.
