# Changelog

## CHANGELOG

### 2.0.0 — 2026-11-27

- Pacing now uses `remaining / days_left`, recomputed at each day boundary.
- Today's allowance is fixed for the day and stored in the snapshot.
- Borrow is additive, percentage-only, and capped by the rest-of-week budget.
- Removed the week-scope borrow option.
- Statusline replaces the extra-of-weekly suffix with `next N days X%/day`.
- `headroom borrow` is exempt from the guard through a strict allowlist.
- Snapshot windows survive a transient statusline omission.
- Added `headroom doctor`; `borrow status` no longer fails on missing windows.
- Removed the daily-percent, weekly-target and grace configuration.

## 1.1.3 — 2026-11-27

- Borrow, setup and doctor commands are exempt from the weekly guard through a
  strict command allowlist.
- Statusline snapshots preserve a window omitted by a transient payload.
- `borrow status` reports missing windows without failing.
- Added `headroom doctor` for paths, snapshot, credit and settings.

### 1.1.2 — 2026-11-26

- Hooks fail open with a setup notice when the plugin binary has not been
  installed yet.

### 1.1.1 — 2026-11-26

- Windows-specific borrow paths in hook messages.
- LF checksum files for Windows release assets.

### 1.1.0 — 2026-11-26

- Native Windows support: `x86_64-pc-windows-msvc` release asset and
  `scripts/install.ps1`.
- Windows CI coverage.
- The release workflow publishes macOS, Linux, and Windows assets and verifies
  checksums in the installers.

### 1.0.0 — 2026-11-26

- Weekly usage pacing with day-N cumulative allowances and carry-forward.
- Compact status line: weekly usage, circular context gauge, today's allowance
  on a 100% scale, weekly-point left/over amount, and borrow row.
- Interactive borrow panel with one-Enter "get back under", presets, custom
  amount, today/week scope, and clear.
- Additive, auto-expiring borrow credits stored separately from the usage
  snapshot.
- PreToolUse and UserPromptSubmit enforcement with a separate 5-hour ceiling.
- Native Rust binary; no runtime dependency.
- Prebuilt macOS, Linux, and Windows release binaries with SHA-256
  verification; the installer downloads the matching asset.
- Runtime contract and architecture decision records.
