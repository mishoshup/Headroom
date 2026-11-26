# ADR 0001: a Rust binary with a flat snapshot and a separate credit file

## Status

Accepted.

## Context

The plugin runs inside two hot paths:

- `PreToolUse` fires before every tool call.
- The status line refreshes on events and on a timer.

The first version measured 27 ms per hook in Bash with multiple `jq`, `date`,
and `stat` processes. That is acceptable relative to model latency but wasteful,
and it requires Bash, jq, and correct shell quoting on every invocation. The
project must remain easy to audit, run on macOS, Linux, and WSL, and have no
service to operate.

## Decision

1. **Rust, one binary.** `pacer` is a single Rust binary with subcommands for
   the status line, both hooks, the borrow panel, and the CLI. Startup is about
   2 ms; there is no runtime dependency.
2. **A flat JSON snapshot for usage.** The status line writes one small JSON
   object containing the two windows. Hooks read it with serde. Counter updates
   are atomic (temporary file plus rename).
3. **A separate credit file.** Borrow credits are not stored in the snapshot,
   because the status line replaces the snapshot every tick. The credit file is
   bound to the weekly reset and, for day credits, to the allowance day.
4. **Fail open.** Missing or stale data allows work. A broken meter must not
   prevent the editor from functioning.
5. **crossterm only for the interactive panel.** It removes more hand-written
   terminal handling than it adds, and it has no effect on the hook path.

## Alternatives

- **Bash + jq.** Rejected as the primary implementation after measurement.
  Kept as a distribution model in the first iteration only.
- **Bun/TypeScript.** Fast enough as a script, but either requires a Bun runtime
  on the user's machine or ships an approximately 85 MB compiled binary.
- **SQLite or a daemon.** Rejected: the state is five numbers and one credit.
  A daemon adds supervision, IPC, and failure modes disproportionate to the
  task.
- **A single combined state file.** Rejected because the status line overwrites
  the snapshot on every tick and would erase the credit.

## Consequences

- A build step (`cargo build --release`) is required at install time. CI
  publishes a release build per platform if distribution expands.
- The installer must place the binary at a stable path because the plugin
  hooks invoke `${CLAUDE_PLUGIN_ROOT}/bin/usage-pacer` and the status line
  invokes `~/.claude/usage-pacer/usage-pacer`.
- The command surface and file schemas are documented in `docs/CONTRACT.md`.
- Reviewers must keep the hook path free of panics and unbounded work; the
  release profile uses `panic = "abort"`.

## Reconsider when

- Enforcement must cover multiple machines or accounts, at which point a
  service or gateway owns the state.
- The plugin grows into a long-lived process with subscriptions or scheduling.
- A platform without Rust release builds becomes a first-class target without a
  prebuilt binary.
