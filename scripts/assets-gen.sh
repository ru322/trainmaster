#!/usr/bin/env bash
# Usage: assets-gen.sh [--check]
# --check regenerates, compares with the previous contents (tracked by git or not), then restores them.
set -euo pipefail

cd "$(dirname "$0")/.."

if command -v flock >/dev/null; then
  exec 9<scripts/assets-gen.sh
  flock 9
else
  echo "warning: flock not found; do not run this script concurrently" >&2
fi

generated=crates/tm-assets/generated
check=false
case "${1:-}" in
  "") ;;
  --check) check=true ;;
  *)
    echo "usage: $0 [--check]" >&2
    exit 2
    ;;
esac

if $check; then
  before=$(mktemp -d)
  restore() {
    rm -rf "$generated"
    mkdir -p "$generated"
    cp -a "$before/." "$generated/"
    rm -rf "$before"
  }
  cp -a "$generated/." "$before/"
  trap restore EXIT
fi

# Emptied first so that output a tool no longer writes cannot linger (03 BD20).
rm -rf "$generated"
mkdir -p "$generated"

cargo run --locked -p tm-spritegen -- build --dir assets/sprites --atlas assets/atlas.toml --out "$generated"
cargo run --locked -p tm-fontgen -- build --config assets/fonts/fontgen.toml --out "$generated"
cargo run --locked -p tm-audiogen -- build --in assets/audio --out "$generated/audio"

if $check; then
  diff -r --brief "$before" "$generated" || {
    echo "assets:check: generated assets are out of date; run 'mise run assets:gen' and commit" >&2
    exit 1
  }
  echo "assets:check: up to date"
fi
