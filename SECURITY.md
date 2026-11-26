# Security policy

## Supported versions

The 1.x series receives security fixes.

## Reporting

Report vulnerabilities privately through GitHub Security Advisories on
`mishoshup/headroom`. Do not open a public issue for a vulnerability.

Include the affected version, the platform, the command or hook involved, and a
minimal reproduction.

## Scope

`headroom` runs locally as a Claude Code status-line command and as
PreToolUse / UserPromptSubmit hooks. It does not make network requests and does
not read credentials. Its state is limited to:

- `~/.claude/headroom/state` — usage snapshot
- `~/.claude/headroom/credit.json` — borrow credit
- `~/.claude/settings.json` — status-line configuration, written by the
  installer with a timestamped backup

The plugin binary is built from this repository. Users who install from a fork
should verify the source they are building.
