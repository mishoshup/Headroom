#!/usr/bin/env bash
# headroom installer: build the binary, install a stable copy under
# ~/.claude, and point Claude Code's statusLine at it.
#
# Flags:
#   --force   replace an existing statusLine that is not headroom

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="${HEADROOM_HOME:-$HOME/.claude/headroom}"
DEST="$DEST_DIR/headroom"
SETTINGS="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json"
REFRESH="${HEADROOM_REFRESH:-60}"
FORCE=0

for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    *) echo "headroom: unknown option $arg" >&2; exit 64 ;;
  esac
done

command -v jq >/dev/null 2>&1 || { echo "headroom: jq is required"; exit 1; }

case "$REFRESH" in ''|*[!0-9]*) echo "headroom: HEADROOM_REFRESH must be whole seconds"; exit 1 ;; esac
((REFRESH >= 1)) || { echo "headroom: HEADROOM_REFRESH must be at least 1"; exit 1; }

mkdir -p "$DEST_DIR"
# macOS kills new executions of a Mach-O file that was overwritten in place
# while a process was running from it. Rename a fresh inode into position.
install_binary() {
  local src=$1 dest=$2 tmp
  tmp="$dest.tmp.$$"
  cp "$src" "$tmp"
  chmod +x "$tmp"
  mv -f "$tmp" "$dest"
}

mkdir -p "$ROOT/bin"
if command -v cargo >/dev/null 2>&1; then
  cargo build --release --manifest-path "$ROOT/Cargo.toml"
  install_binary "$ROOT/target/release/headroom" "$ROOT/bin/headroom"
elif [ -x "$ROOT/bin/headroom" ]; then
  :
else
  echo "headroom: cargo is required to build, or place a prebuilt binary at bin/headroom" >&2
  exit 1
fi

mkdir -p "$DEST_DIR"
install_binary "$ROOT/bin/headroom" "$DEST"
"$DEST" self-test >/dev/null

[ -f "$SETTINGS" ] || printf '{}\n' > "$SETTINGS"
existing=$(jq -r '.statusLine.command // ""' "$SETTINGS")

if [ -n "$existing" ] && [[ $existing != *"$DEST"* ]] && ((FORCE == 0)); then
  echo "headroom: a different statusLine is already configured:"
  echo "  $existing"
  echo "headroom: leaving it alone. Re-run with --force to replace it, or set:"
  echo "  \"statusLine\": {\"type\": \"command\", \"command\": \"$DEST statusline\", \"refreshInterval\": $REFRESH, \"padding\": 1}"
  exit 0
fi

backup="$SETTINGS.bak.$(date +%s)"
cp "$SETTINGS" "$backup"

tmp="$SETTINGS.tmp.$$"
jq --arg cmd "$DEST statusline" --argjson ri "$REFRESH" \
  '.statusLine = {type: "command", command: $cmd, refreshInterval: $ri}' \
  "$SETTINGS" > "$tmp" && mv -f "$tmp" "$SETTINGS"

echo "headroom: installed $DEST"
echo "headroom: statusline wired (refresh ${REFRESH}s); settings backup at $backup"
echo "headroom: borrow panel:  $DEST"
echo "headroom: borrow CLI:    $DEST borrow 20"
echo "headroom: restart Claude Code to load the hooks and statusline"
