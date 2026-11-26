# Headroom

[![CI](https://github.com/mishoshup/headroom/actions/workflows/ci.yml/badge.svg)](https://github.com/mishoshup/headroom/actions/workflows/ci.yml)

Weekly usage pacing for Claude Code, maintained by
[Mishoshup](https://github.com/mishoshup).

Headroom enforces a daily share of the weekly subscription allowance and
reports how much of today's allowance remains. It is a single native Rust
binary with no runtime dependencies.

![Headroom status line and interactive borrow panel](docs/panel.png)

## Overview

Claude Code exposes the weekly rate-limit percentage and its reset time to the
status-line command. `headroom` divides the seven-day window into daily
allowances:

```
window_start = resets_at - 7 days
day          = floor((now - window_start) / 1 day) + 1
allowance    = daily_percent × day
```

Day N receives N daily shares, so unused allowance carries forward. The
default daily share is one seventh of the weekly allowance.

The plugin provides two surfaces:

1. A concise status line reporting weekly usage and today's position against
   its allowance.
2. An interactive panel for the detailed view and for borrowing additional
   headroom.

## Status line

```
Usage · Sonnet · context ◔ 12%
weekly  ▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱     46%  resets in 4d 12h
today   ▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰  107.3%  3.13% extra of weekly
```

The header and the bar rows share an eight-column label gutter, so `Sonnet`
and every bar begin on the same column.

The `today` row uses a 100% scale for today's allowance:

- `100%` means the allowance is exactly consumed.
- Values above `100%` indicate overspend, for example `120%`.
- The trailing figure expresses the same position in weekly percentage
  points: `3.13% extra of weekly`, or `12.16% left of weekly`.

When a borrow is active, one subordinate row reports it:

```
          +20% borrowed · allowance 62.87% of weekly
```

## Interactive panel

Run `~/.claude/headroom/headroom` without arguments, or
`~/.claude/headroom/headroom borrow`, in a terminal.

```
Headroom  borrow extra headroom
You are 3.13% over today's limit.  Weekly usage is 46%.
────────────────────────────────────────────────
weekly used ▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱    46%  resets in 4d 12h
today limit ▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰  42.87%  3.13% over

Borrow to get back under today's limit
▸ +5%      gets you under   → limit 47.87% · 1.87% left
  +10%    → limit 52.87% · 6.87% left
  +15%    → limit 57.87% · 11.87% left
  +20%    → limit 62.87% · 16.87% left
  +25%    → limit 67.87% · 21.87% left
  +30%                      → limit 72.87% · 26.87% left
  custom amount…
  clear current borrow
  quit

Selected: add +5%  →  limit 47.87%  ·  1.87% left  ·  expires in 12h 0m

↑/↓ choose · enter confirm · w scope · c clear · q quit
```

Behaviour:

- When usage is above the allowance, the first item is the smallest borrow
  that returns the account below the limit.
- Every option previews the resulting limit and the remaining or excess
  amount.
- `w` toggles the borrow scope between today and the remainder of the week.
- `enter` applies the selected option; `c` clears the current borrow; `q`
  exits.

## Borrowing headroom

A borrow adds percentage points of the weekly limit to the current allowance.
Borrows are additive and expire automatically.

```bash
~/.claude/headroom/headroom borrow 20
```

```
headroom: added +20% for today.
today's limit: 42.87% -> 62.87%.
You now have 16.87% left today.
Expires in 11h 59m. Future days keep their normal limits, so unused
allowance repays the borrow.
```

Options:

- `borrow 20` — add 20 points until the next day boundary.
- `borrow 15 --week` — add 15 points until the weekly reset.
- `borrow 10 --note "off Friday and Saturday"` — attach a note.
- `borrow status` — display the active borrow.
- `borrow clear` — remove it.

The credit is stored in `~/.claude/headroom/credit.json`, keyed to the
weekly reset and day. Inside Claude Code, `/headroom:borrow 20` and
`!~/.claude/headroom/headroom borrow 20` provide the same command.

## Enforcement

When weekly usage is above the current allowance:

- `PreToolUse` denies tool calls and reports today's limit and the next
  allowance time.
- `UserPromptSubmit` blocks new prompts. Slash commands and `!` shell commands
  remain available so that usage can be inspected and the allowance can be
  adjusted.
- The status line marks the `today` row in red and states the excess.

A separate 5-hour ceiling is enforced at 98% by default.

## Installation

Requirements:

- Claude Code 2.1 or later with a Pro, Max, or Ultra subscription
- Rust 1.74 or later to build; `jq` for the installer
- macOS, Linux, or WSL

```bash
claude plugin marketplace add mishoshup/headroom
claude plugin install headroom@mishoshup
```

Then run `/headroom:setup` inside Claude Code. For a local checkout:

```bash
claude plugin marketplace add ~/Developer/headroom
claude plugin install headroom@mishoshup
bash ~/Developer/headroom/scripts/install.sh
```

The installer builds the release binary, installs it under
`~/.claude/headroom/`, copies the hook binary into the plugin directory,
takes a timestamped backup of `settings.json`, and configures the status line.
Claude Code must be restarted after installation so that the hooks are loaded.

If a different status line is already configured, the installer leaves it
untouched. Pass `--force` to replace it.

The binary is installed at `~/.claude/headroom/headroom` and is **not**
added to `PATH`. The hooks invoke it by absolute path, and all borrow commands
in this document use that absolute path.

## Configuration

Environment variables are read by the binary at invocation. They must be set
in the environment that launches Claude Code.

| Variable | Default | Purpose |
|---|---|---|
| `HEADROOM_DAILY_PERCENT` | `100 / 7` (14.29) | Percentage points of the weekly limit granted per day. |
| `HEADROOM_WEEKLY_TARGET` | `100` | Weekly percentage to distribute when `HEADROOM_DAILY_PERCENT` is unset. |
| `HEADROOM_PACING` | `on` | Set to `off` to disable the weekly pace guard. |
| `HEADROOM_PACING_GRACE` | `0` | Additional points of tolerance before blocking. |
| `HEADROOM_BLOCK_PROMPTS` | `1` | Set to `0` to block tools only. |
| `HEADROOM_FIVE_HOUR_LIMIT` | `98` | Five-hour ceiling, in percent. |
| `HEADROOM_FIVE_HOUR_MODE` | `block` | `wait` sleeps until the five-hour reset instead of denying. |
| `HEADROOM_STALE_AFTER` | `900` | Maximum snapshot age, in seconds. |
| `HEADROOM_WEEK_DAYS` | `7` | Window length, in days. |
| `HEADROOM_COLOR` | automatic | `off`, `16`, `256`, or `truecolor`. `NO_COLOR` also disables colour. |
| `HEADROOM_ASCII` | `0` | Set to `1` for ASCII bars and glyphs. |
| `HEADROOM_BOTTOM_MARGIN` | `1` | Blank rows between the status line and the built-in footer (0–3). |
| `HEADROOM_STATE` | `~/.claude/headroom/state` | Snapshot path. |
| `HEADROOM_CREDIT` | `~/.claude/headroom/credit.json` | Borrow-credit path. |

## Failure behaviour

- The guard fails open when the snapshot is absent, malformed, or older than
  `HEADROOM_STALE_AFTER`. A broken meter must not prevent work.
- The status line is the source of the rate-limit snapshot; if the status
  line is not configured, enforcement is inactive.
- State and credit writes are atomic (temporary file followed by rename).
- Day boundaries are anchored to the weekly reset instant, not local
  midnight.

## Development

```
src/
  main.rs        subcommand dispatch
  config.rs      environment configuration and colour detection
  state.rs       usage snapshot with atomic persistence
  pace.rs        allowance arithmetic
  credit.rs      borrow persistence and CLI
  render.rs      status-line rendering
  style.rs       truecolor, 256-colour, 16-colour, and plain output
  hook.rs        PreToolUse and UserPromptSubmit enforcement
  statusline.rs  status-line input handling
  ui.rs          interactive borrow panel
hooks/hooks.json command registrations
scripts/install.sh build, install, settings backup, status-line wiring
tests/cli.rs     end-to-end behaviour tests
```

The runtime contract is documented in [docs/CONTRACT.md](docs/CONTRACT.md).
Architecture decisions are recorded in
[docs/adr](docs/adr/0001-rust-binary-flat-state.md).

Quality gates:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
```

The crate forbids `unsafe` and denies `todo!`, `unimplemented!`, and `dbg!`.
The minimum supported Rust version is 1.74. CI runs formatting, linting,
tests, and a release build on every push.

## Uninstall

```bash
claude plugin uninstall headroom@mishoshup
claude plugin marketplace remove mishoshup
rm -rf ~/.claude/headroom
```

Remove the `statusLine` key from `~/.claude/settings.json` if another status
line is not configured in its place.

## Licence

MIT © 2026 Mishoshup
