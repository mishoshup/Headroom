---
description: Borrow extra pace allowance for today or for the rest of the weekly window.
argument-hint: "<percent> [--week] [--note text] | status | clear"
disable-model-invocation: true
---

Run the installed binary with the user's arguments and report the output verbatim:

```bash
~/.claude/usage-pacer/usage-pacer borrow $ARGUMENTS
```

- No arguments: run `~/.claude/usage-pacer/usage-pacer borrow status`.
- Examples: `borrow 20`, `borrow 15 --week`, `borrow clear`.
- The interactive panel is for a real terminal: run `usage-pacer` or
  `usage-pacer borrow` there. Inside Claude Code's Bash tool it prints status.
- The credit is stored in `~/.claude/usage-pacer/credit.json` and expires automatically at the next day boundary (or at the weekly reset with `--week`).
