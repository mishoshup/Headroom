---
description: Borrow part of the rest of the week into today's allowance.
argument-hint: "<percent> [--note text] | status | clear"
disable-model-invocation: true
---

Run the installed binary with the user's arguments and report the output verbatim:

```bash
~/.claude/headroom/headroom borrow $ARGUMENTS
```

- No arguments: run `~/.claude/headroom/headroom borrow status`.
- On Windows, the binary is `%USERPROFILE%\.claude\headroom\headroom.exe`; use that path instead of the tilde path.
- Examples: `borrow 20`, `borrow clear`.
- The credit adds to today's fixed allowance, capped by what remains for the rest of the week. It expires at the next day boundary.
- This command is exempt from the weekly guard, so `/headroom:borrow 20` works while today's allowance is used.
