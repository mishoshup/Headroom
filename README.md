# usage-pacer

A compiled Claude Code plugin that paces subscription usage across the weekly
window instead of letting it burn out in a day.

Claude Code reports a `seven_day` percentage and a reset time. The plugin
treats the seven days before that reset as the budget window and gives day N
an allowance of `daily_percent × N`:

```
window_start = resets_at - 7 days
day          = floor((now - window_start) / 1 day) + 1     # 1..7
ceiling      = min(100, daily_percent × day) + grace + borrowed
```

Unused allowance carries forward; it is not lost at midnight. Day 1 starts
with a full day of headroom, so a fresh window never blocks immediately.

```
Usage · Sonnet · ctx 42.4%
5h          ▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱     12%  ↻59m
7d          ▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱▱▱▱▱     45%  ↻3d 11h
pace d4/7   ▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱  57.16% cap  12.16 left
```

The statusline is rendered by a single Rust binary: truecolor when the
terminal advertises it, 256/16-color fallback, `NO_COLOR` and ASCII modes,
and bar width derived from `COLUMNS`. Cold start is ~2 ms.

## Borrowing headroom

Sometimes you know a heavy day is followed by quiet ones. Borrow the
difference instead of disabling the guard:

```bash
~/.claude/usage-pacer/usage-pacer borrow 20
```

```
usage-pacer: borrowed +20% for today.
day 4/7 ceiling: 57.16% -> 77.16% (max 100%).
Expires in 11h 59m. Future days keep their normal ceilings, so unused
allowance repays the borrow.
```

- `borrow 20` — add 20 percentage points until the next day boundary.
- `borrow 15 --week` — add 15 points until the weekly reset.
- `borrow 10 --note "off Fri-Sat"` — attach a note to the credit.
- `borrow status` — show the active override.
- `borrow clear` — remove it.

Credits live in `~/.claude/usage-pacer/credit.json`, keyed to the weekly
reset and day, so they expire on their own. Future days keep their normal
ceilings; the carry-forward pace absorbs the debt naturally. Inside Claude
Code you can use `/usage-pacer:borrow 20` or `!~/.claude/usage-pacer/usage-pacer borrow 20`.

## Enforcement

When weekly usage is above the ceiling:

- `PreToolUse` denies tool calls, naming the day, ceiling and unlock time.
- `UserPromptSubmit` blocks new prompts (`/` and `!` always pass so you can
  inspect usage, borrow, or recover).
- The pace row turns red and shows how far over you are.

A separate 5-hour ceiling is enforced at the same time (default 98%).

## Requirements

- Claude Code 2.1+ with a Pro or Max subscription (rate-limit data is not
  exposed for API-key auth)
- Rust 1.70+ to build (there is no runtime dependency)
- `jq` for the installer only
- macOS, Linux, or WSL

## Install

```bash
claude plugin marketplace add ~/Developer/usage-pacer
claude plugin install usage-pacer@usage-pacer-local
bash ~/Developer/usage-pacer/scripts/install.sh
```

Or run `/usage-pacer:setup` inside Claude Code. The installer builds the
release binary, copies it to `~/.claude/usage-pacer/usage-pacer` and
`<plugin>/bin/usage-pacer` (used by the hooks), backs up `settings.json`,
and wires the statusline with `padding: 1`. Restart Claude Code afterwards.

If a different statusline is already configured, the installer leaves it
alone; use `--force` to replace it.

## Configuration

Set these in the environment that launches Claude Code, then restart.

| Variable | Default | Meaning |
|---|---|---|
| `PACER_DAILY_PERCENT` | `100 / 7` (14.29) | Percentage points of the weekly limit per day. |
| `PACER_WEEKLY_TARGET` | `100` | Weekly percent to spread when `PACER_DAILY_PERCENT` is unset. |
| `PACER_PACING` | `on` | `off` disables the weekly pace guard. |
| `PACER_PACING_GRACE` | `0` | Extra points of slack before blocking. |
| `PACER_BLOCK_PROMPTS` | `1` | `0` blocks tools only, not new prompts. |
| `PACER_FIVE_HOUR_LIMIT` | `98` | 5-hour ceiling, in percent. |
| `PACER_FIVE_HOUR_MODE` | `block` | `wait` sleeps until the 5-hour reset instead. |
| `PACER_STALE_AFTER` | `900` | Ignore snapshots older than this many seconds. |
| `PACER_WEEK_DAYS` | `7` | Window length in days. |
| `PACER_COLOR` | auto | `off`, `16`, `256`, `truecolor`; `NO_COLOR` also disables. |
| `PACER_ASCII` | `0` | `1` for ASCII bars/glyphs. |
| `PACER_STATE` | `~/.claude/usage-pacer/state` | Snapshot path. |
| `PACER_CREDIT` | `~/.claude/usage-pacer/credit.json` | Override credit path. |

Examples:

```bash
export PACER_DAILY_PERCENT=12   # 84% of the weekly limit, spread evenly
export PACER_DAILY_PERCENT=15   # 100% by day 7 (clamped), roomier early days
export PACER_ASCII=1            # only ASCII glyphs
claude
```

## How it behaves

- **Fail open.** Missing, malformed or stale data allows work. A broken meter
  must not brick the editor.
- **The statusline is the data source.** Claude Code exposes `rate_limits`
  only to the statusline command, so hooks read the snapshot it writes. If
  the statusline is not wired, nothing is enforced.
- **Atomic state.** One JSON snapshot and one credit file, written via
  temp-file + rename.
- **Day boundaries are anchored to the reset instant**, not local midnight.
- **Subagents** share the same snapshot and ceilings.
- **Recovery:** `/`, `!`, `borrow`, `borrow clear`, or
  `claude plugin disable usage-pacer@usage-pacer-local`.

## Files

```
src/
  main.rs        subcommand dispatch
  config.rs      env config + colour capability detection
  state.rs       usage snapshot (serde, atomic write)
  pace.rs        day/ceiling maths + borrow credit
  credit.rs      override persistence and CLI
  render.rs      adaptive /context-style dashboard
  style.rs       truecolor / 256 / 16 / plain SGR
  hook.rs        pretool + prompt enforcement
  statusline.rs  stdin JSON -> snapshot -> dashboard
hooks/hooks.json PreToolUse + UserPromptSubmit registrations
scripts/install.sh  build, install, settings backup, statusline wiring
tests/cli.rs     end-to-end behaviour tests
```

## Tests

```bash
cargo test --release      # unit + CLI integration tests
cargo run --release -- self-test
```

## Development standards

- Formatting: `cargo fmt --check` (rustfmt config in `rustfmt.toml`).
- Lints: `cargo clippy --all-targets -- -D warnings`; `unsafe` is forbidden and
  `todo!`/`unimplemented!`/`dbg!` are denied at the crate level.
- MSRV: Rust 1.74.
- Runtime paths must not panic: hooks fail open on any parse/IO error, and
  test-only `unwrap` is kept out of `main`/hook/statusline code.
- `cargo test --release` covers pace maths, credit expiry, rendering modes and
  each CLI subcommand end to end.
- CI runs fmt, clippy, tests and a release build on every push.

## Uninstall

```bash
claude plugin uninstall usage-pacer@usage-pacer-local
claude plugin marketplace remove usage-pacer-local
rm -rf ~/.claude/usage-pacer
```

Remove the `statusLine` key from `~/.claude/settings.json` if you do not
replace it with another one.

## License

MIT © 2026 danialhaikal
