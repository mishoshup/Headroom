---
description: Build and install usage-pacer, then wire Claude Code's statusline to it.
disable-model-invocation: true
---

Run:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/install.sh"
```

Report what it prints.

- It builds the release binary, copies it to `~/.claude/usage-pacer/usage-pacer`, installs a copy for the plugin hooks, and only touches the `statusLine` key in `settings.json` after taking a timestamped backup.
- If it reports an existing statusline was left alone, relay the exact `--force` instructions.
- Re-running after an update rebuilds and refreshes both copies.
