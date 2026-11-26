---
description: Build and install headroom, then wire Claude Code's statusline to it.
disable-model-invocation: true
---

Run:

```bash
bash "${CLAUDE_PLUGIN_ROOT}/scripts/install.sh"
```

Report what it prints.

- The installer downloads the matching prebuilt release binary and verifies its
  SHA-256 checksum. No Rust toolchain is required.
- It installs `~/.claude/headroom/headroom`, copies the hook binary into the
  plugin directory, and calls `headroom setup` to wire `settings.json` after a
  timestamped backup.
- If it reports an existing statusline was left alone, relay the exact
  `--force` instructions.
- If the platform has no prebuilt asset, it falls back to a source build when
  Cargo is available.
