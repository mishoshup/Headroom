#!/usr/bin/env bash
# usage-pacer installer: build the binary, install a stable copy under
# ~/.claude, and point Claude Code's statusLine at it.
#
# Flags:
#   --force   replace an existing statusLine that is not usage-pacer

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="${PACER_HOME:-$HOME/.claude/usage-pacer}"
DEST="$DEST_DIR/usage-pacer"
SETTINGS="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json"
REFRESH="${PACER_REFRESH:-60}"
FORCE=0

for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    *) echo "usage-pacer: unknown option $arg" >&2; exit 64 ;;
  esac
done

command -v jq >/dev/null 2>&1 || { echo "usage-pacer: jq is required"; exit 1; }

case "$REFRESH" in ''|*[!0-9]*) echo "usage-pacer: PACER_REFRESH must be whole seconds"; exit 1 ;; esac
((REFRESH >= 1)) || { echo "usage-pacer: PACER_REFRESH must be at least 1"; exit 1; }

mkdir -p "$ROOT/bin"
if command -v cargo >/dev/null 2>&1; then
  cargo build --release --manifest-path "$ROOT/Cargo.toml"
  cp "$ROOT/target/release/usage-pacer" "$ROOT/bin/usage-pacer"
elif [ -x "$ROOT/bin/usage-pacer" ]; then
  :
else
  echo "usage-pacer: cargo is required to build, or place a prebuilt binary at bin/usage-pacer" >&2
  exit 1
fi
chmod +x "$ROOT/bin/usage-pacer"

mkdir -p "$DEST_DIR"
cp "$ROOT/bin/usage-pacer" "$DEST"
chmod +x "$DEST"
"$DEST" self-test >/dev/null

[ -f "$SETTINGS" ] || printf '{}\n' > "$SETTINGS"
existing=$(jq -r '.statusLine.command // ""' "$SETTINGS")

if [ -n "$existing" ] && [[ $existing != *"$DEST"* ]] && ((FORCE == 0)); then
  echo "usage-pacer: a different statusLine is already configured:"
  echo "  $existing"
  echo "usage-pacer: leaving it alone. Re-run with --force to replace it, or set:"
  echo "  \"statusLine\": {\"type\": \"command\", \"command\": \"$DEST statusline\", \"refreshInterval\": $REFRESH, \"padding\": 1}"
  exit 0
fi

backup="$SETTINGS.bak.$(date +%s)"
cp "$SETTINGS" "$backup"

tmp="$SETTINGS.tmp.$$"
jq --arg cmd "$DEST statusline" --argjson ri "$REFRESH" \
  '.statusLine = {type: "command", command: $cmd, refreshInterval: $ri}' \
  "$SETTINGS" > "$tmp" && mv -f "$tmp" "$SETTINGS"

echo "usage-pacer: installed $DEST"
echo "usage-pacer: statusline wired (refresh ${REFRESH}s); settings backup at $backup"
echo "usage-pacer: restart Claude Code to load the hooks and statusline"
