---
name: Bug report
about: Something broke or behaved unexpectedly
labels: bug
---

## What happened

<!-- Describe the actual behaviour you observed. -->

## What you expected

<!-- Describe what you thought should happen instead. -->

## Steps to reproduce

1. ...
2. ...
3. ...

## Environment

- Convoy version: <!-- `convoy version` -->
- OS: <!-- macOS 15.x / Ubuntu 24.04 / ... -->
- Rust version: <!-- `rustc --version` -->
- Claude Code version (if relevant):

## Diagnostics

<!-- Helpful, NOT required: -->

- `convoy doctor` output:

  ```
  paste here
  ```

- `convoy status` output (when the bug occurred):

  ```
  paste here
  ```

- Last ~50 lines of `~/.convoy/daemon.log`:

  ```
  paste here
  ```

- `convoy export <project-path> --redacted --to /tmp/bug-bundle` and attach
  the bundle if state inspection is needed (the `--redacted` flag masks
  absolute paths).

## Additional context

<!-- Anything else? Screenshots, related PRs, prior issues, hunches. -->
