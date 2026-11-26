# Runtime contract

This document is the source of truth for the interfaces `usage-pacer` consumes
and produces. Changes to these shapes are contract changes and require a
matching update here.

## 1. Command-line interface

| Command | Output | Exit code |
|---|---|---|
| `usage-pacer` / `usage-pacer ui` | Interactive borrow panel on a TTY; status text otherwise | 0, 1 on failure |
| `usage-pacer statusline` | Status-line text on stdout | 0 |
| `usage-pacer pretool` | Nothing, or a PreToolUse deny object on stdout | 0 |
| `usage-pacer prompt` | Context on stdout, or a block message on stderr | 0, 2 when blocked |
| `usage-pacer borrow <pct> [--week] [--note text]` | Confirmation text | 0, 1 on failure |
| `usage-pacer borrow status` | Active credit text | 0, 1 on failure |
| `usage-pacer borrow clear` | Confirmation text | 0, 1 on failure |
| `usage-pacer self-test` | `self-test passed` | 0, 1 on failure |
| `usage-pacer help` | Usage text | 0 |

`borrow` is additive. The resulting credit is clamped to 100% of the weekly
limit. A day-scoped credit expires at the next allowance boundary; a
week-scoped credit expires at the weekly reset.

## 2. Status-line input

Claude Code provides a JSON object on stdin. The following fields are used:

| Field | Type | Use |
|---|---|---|
| `model.display_name` | string | Header model label |
| `context_window.used_percentage` | number, nullable | Context gauge |
| `rate_limits.five_hour.used_percentage` | number, optional | 5-hour ceiling |
| `rate_limits.five_hour.resets_at` | epoch seconds, optional | 5-hour reset |
| `rate_limits.seven_day.used_percentage` | number, optional | Weekly usage |
| `rate_limits.seven_day.resets_at` | epoch seconds, optional | Weekly window anchor |

Absent windows are valid. An unparseable payload produces the minimal header
`Claude` and leaves the previous snapshot untouched.

## 3. Hook output

`pretool` emits nothing when the call is allowed. When denied:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "usage-pacer: ..."
  }
}
```

`prompt` exits `0` with optional context, or exits `2` with a block message on
stderr. Slash commands and `!` shell commands are never blocked.

## 4. Snapshot (`state`)

Atomic JSON, written by `statusline` and read by the hooks.

```json
{
  "captured_at": 1795678885,
  "five_hour": { "used_bp": 1200, "resets_at": 1795682485 },
  "seven_day": { "used_bp": 4600, "resets_at": 1796023200 }
}
```

- `used_bp` is the percentage in basis points (`4600` = 46.00%).
- `resets_at` is Unix epoch seconds.
- Either window may be absent.
- A snapshot older than `PACER_STALE_AFTER` is ignored, which fails open.

## 5. Borrow credit (`credit.json`)

```json
{
  "window_reset": 1796023200,
  "day": 3,
  "extra_bp": 2000,
  "expires_at": 1795700000,
  "scope": "day",
  "note": "off Friday",
  "created_at": 1795678885
}
```

- `scope` is `day` or `week`.
- `window_reset` binds the credit to one weekly window.
- `day` binds a day-scoped credit to one allowance day.
- `expires_at` is authoritative; an expired credit is ignored.
- The file is separate from the snapshot so status-line refreshes cannot
  erase it.

## 6. Allowance invariants

```
week_seconds = PACER_WEEK_DAYS * 86400
elapsed      = clamp(now - (resets_at - week_seconds), 0, week_seconds - 1)
day          = elapsed / 86400 + 1
daily_bp     = PACER_DAILY_PERCENT or round(PACER_WEEKLY_TARGET / week_days)
allowance_bp = min(10000, daily_bp * day)
ceiling_bp   = min(10000, allowance_bp + grace_bp + credit_bp)
over         = used_bp > ceiling_bp
```

A day-scoped credit expires at `min(resets_at, window_start + day * 86400)`.

## 7. Failure behaviour

| Failure | Behaviour |
|---|---|
| Missing, stale, or malformed snapshot | Hooks allow work (fail open) |
| Status-line not configured | No snapshot; hooks allow |
| Status-line payload unparseable | Minimal header; previous snapshot retained |
| Snapshot write failure | Status line still renders; next tick retries |
| Credit write failure | CLI/panel reports the error and exits non-zero |
| Wait mode reset longer than `PACER_MAX_SLEEP` | Tool call denied; no partial sleep |

The guard is local to the machine and the Claude Code process. It is not a
billing system: `claude --bare`, environment overrides, another machine, and
API use outside Claude Code are outside its authority.

## 8. Versioning

The snapshot and credit files are unversioned. New fields must be optional and
serde-defaulted; existing fields must not change meaning. Commands may gain
flags, but existing flags keep their behaviour.
