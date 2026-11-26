# Changelog

## CHANGELOG

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
