#!/usr/bin/env bash
# headroom installer: fetch a prebuilt release binary (or build with --build),
# install a stable copy under ~/.claude, and wire the status line.
#
# Flags:
#   --force   replace an existing statusLine that is not headroom
#   --build   build from source instead of downloading the release
#
# Environment:
#   HEADROOM_HOME      install directory (default ~/.claude/headroom)
#   HEADROOM_REFRESH   statusline refresh seconds (default 60)
#   HEADROOM_BINARY    use this binary instead of downloading/building

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="${HEADROOM_HOME:-$HOME/.claude/headroom}"
DEST="$DEST_DIR/headroom"
REFRESH="${HEADROOM_REFRESH:-60}"
REPO="mishoshup/Headroom"
FORCE=0
BUILD=0

for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    --build) BUILD=1 ;;
    *) echo "headroom: unknown option $arg" >&2; exit 64 ;;
  esac
done

case "$REFRESH" in ''|*[!0-9]*) echo "headroom: HEADROOM_REFRESH must be whole seconds"; exit 1 ;; esac
((REFRESH >= 1)) || { echo "headroom: HEADROOM_REFRESH must be at least 1"; exit 1; }
export HEADROOM_REFRESH="$REFRESH"

install_binary() {
  local src=$1 dest=$2 tmp
  tmp="$dest.tmp.$$"
  cp "$src" "$tmp"
  chmod +x "$tmp"
  mv -f "$tmp" "$dest"
}

detect_target() {
  case "$(uname -s):$(uname -m)" in
    Darwin:arm64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    Linux:x86_64) echo "x86_64-unknown-linux-gnu" ;;
    Linux:aarch64|Linux:arm64) echo "aarch64-unknown-linux-gnu" ;;
    *) echo "" ;;
  esac
}

fetch() {
  local url=$1 dest=$2
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$dest"
  else
    return 1
  fi
}

download_prebuilt() {
  local target=$1 tmp asset url
  [ -n "$target" ] || return 1
  command -v tar >/dev/null 2>&1 || return 1

  tmp="$(mktemp -d)"
  asset="headroom-$target.tar.gz"
  url="https://github.com/$REPO/releases/latest/download/$asset"

  if ! fetch "$url" "$tmp/$asset"; then
    rm -rf "$tmp"
    return 1
  fi
  if ! fetch "$url.sha256" "$tmp/$asset.sha256"; then
    rm -rf "$tmp"
    return 1
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$tmp" && sha256sum -c "$asset.sha256" >/dev/null) || { rm -rf "$tmp"; return 1; }
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$tmp" && shasum -a 256 -c "$asset.sha256" >/dev/null) || { rm -rf "$tmp"; return 1; }
  else
    rm -rf "$tmp"
    return 1
  fi

  tar -xzf "$tmp/$asset" -C "$tmp" || { rm -rf "$tmp"; return 1; }
  [ -x "$tmp/headroom" ] || { rm -rf "$tmp"; return 1; }
  install_binary "$tmp/headroom" "$ROOT/bin/headroom"
  rm -rf "$tmp"
  echo "headroom: downloaded release asset $asset"
}

build_from_source() {
  command -v cargo >/dev/null 2>&1 || return 1
  cargo build --release --manifest-path "$ROOT/Cargo.toml"
  install_binary "$ROOT/target/release/headroom" "$ROOT/bin/headroom"
}

target="$(detect_target)"
mkdir -p "$ROOT/bin" "$DEST_DIR"

if [ -n "${HEADROOM_BINARY:-}" ] && [ -x "$HEADROOM_BINARY" ]; then
  install_binary "$HEADROOM_BINARY" "$ROOT/bin/headroom"
elif ((BUILD)); then
  build_from_source || {
    echo "headroom: --build requires the Rust toolchain" >&2
    exit 1
  }
elif ! download_prebuilt "$target"; then
  echo "headroom: no prebuilt binary for '${target:-this platform}'; trying a source build" >&2
  build_from_source || {
    echo "headroom: could not download a release or build from source." >&2
    echo "headroom: install Rust, or set HEADROOM_BINARY to a compatible binary." >&2
    exit 1
  }
fi

install_binary "$ROOT/bin/headroom" "$DEST"
"$DEST" self-test >/dev/null

if ((FORCE)); then
  "$DEST" setup --force
else
  "$DEST" setup
fi

echo "headroom: installed $DEST"
echo "headroom: borrow panel:  $DEST"
echo "headroom: borrow CLI:    $DEST borrow 20"
echo "headroom: restart Claude Code to load the hooks and statusline"
