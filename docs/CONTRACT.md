# Runtime contract

This document is the source of truth for the interfaces `headroom` consumes
and produces. Changes to these shapes are contract changes and require a
matching update here.

## 1. Command-line interface

| Command | Output | Exit code |
|---|---|---|
| `headroom` / `headroom ui` | Interactive borrow panel on a TTY; status text otherwise | 0, 1 on failure |
| `headroom statusline` | Status-line text on stdout | 0 |
| `headroom pretool` | Nothing, or a PreToolUse deny object on stdout | 0 |
| `headroom prompt` | Context on stdout, or a block message on stderr | 0, 2 when blocked |
| `headroom borrow <pct> [--note text]` | Confirmation text | 0, 1 on failure |
| `headroom borrow status` | Active credit text | 0, 1 on failure |
| `headroom borrow clear` | Confirmation text | 0, 1 on failure |
| `headroom setup [--force]` | Status-line configuration message | 0, 1 on failure |
| `headroom doctor` | Paths, snapshot, credit and settings report | 0 |
| `headroom self-test` | `self-test passed` | 0, 1 on failure |
| `headroom help` | Usage text | 0 |

`borrow` is additive. The credit raises today's fixed allowance and is capped
by the amount left for the days after today. It expires at the next allowance
boundary (or the weekly reset on the last day).

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
    "permissionDecisionReason": "headroom: ..."
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
  "seven_day": { "used_bp": 4600, "resets_at": 1796023200 },
  "day": 3,
  "day_cap_bp": 5680,
  "day_reset": 1796023200,
  "week_days": 7
}
```

- `used_bp` is the percentage in basis points (`4600` = 46.00%).
- `resets_at` is Unix epoch seconds.
- `day`, `day_cap_bp`, `day_reset`, and `week_days` pin today's fixed
  allowance to its weekly window.
- Either window may be absent; a transient omission keeps the previous window
  when its reset is still in the future and the snapshot is fresh.
- A snapshot older than `HEADROOM_STALE_AFTER` is ignored, which fails open.

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

- `scope` is `day`. The field remains for compatibility; older `week` credits
  are only honoured on their creation day.
- `window_reset` binds the credit to one weekly window.
- `day` binds the credit to one allowance day.
- `expires_at` is authoritative; an expired credit is ignored.
- The file is separate from the snapshot so status-line refreshes cannot
  erase it.

## 6. Allowance invariants

```
week_seconds = HEADROOM_WEEK_DAYS * 86400
window_start = resets_at - week_seconds
day          = floor((now - window_start) / 86400) + 1
days_left    = week_days - day + 1
remaining    = 10000 - used_bp
base_cap     = used_bp + remaining / days_left
ceiling      = min(10000, base_cap + credit_bp)
left         = max(0, ceiling - used_bp)
future/day   = (10000 - used_bp) / max(1, week_days - day)
```

`base_cap` is fixed for the day and stored in the snapshot (`day_cap_bp`). It is
recomputed only when the day, weekly window or `week_days` changes. The
statusline refreshes it; hooks fall back to the formula if the stored value is
missing or stale.

A borrow raises `ceiling` and is capped at `10000 - base_cap`. It does not
change the total weekly budget; tomorrow divides whatever is actually left over
the remaining days.

## 7. Failure behaviour

| Failure | Behaviour |
|---|---|
| Missing, stale, or malformed snapshot | Hooks allow work (fail open) |
| Status-line not configured | No snapshot; hooks allow |
| Status-line payload unparseable | Minimal header; previous snapshot retained |
| Snapshot write failure | Status line still renders; next tick retries |
| Credit write failure | CLI/panel reports the error and exits non-zero |
| Wait mode reset longer than `HEADROOM_MAX_SLEEP` | Tool call denied; no partial sleep |

The guard is local to the machine and the Claude Code process. It is not a
billing system: `claude --bare`, environment overrides, another machine, and
API use outside Claude Code are outside its authority.

## 8. Versioning

The snapshot and credit files are unversioned. New fields must be optional and
serde-defaulted; existing fields must not change meaning. Commands may gain
flags, but existing flags keep their behaviour.
