#!/bin/bash
# After a clone (and safe to run again): links each workspace crate's target/ to the root's, so the paths scripts and
# docs name (sdk/react/target/release/raea-motion, apps/void/target/release/void, ...) are the one build's. Cargo
# builds every crate into the root target/ (Cargo.toml's workspace); a crate dir that still holds a real target/ from
# before the workspace is left alone and named, to delete when nothing runs from it.
#   tools/setup.sh [ROOT]     ROOT: the checkout (default: this script's)
set -euo pipefail
ROOT=$(cd "${1:-$(dirname "$0")/..}" && pwd)
mkdir -p "$ROOT/target"
# the members, as Cargo.toml lists them
members=$(sed -n '/^members = \[/,/^\]/p' "$ROOT/Cargo.toml" | sed -n 's/^ *"\([^"]*\)",*$/\1/p')
for m in $members; do
  d=$ROOT/$m
  [ -d "$d" ] || continue
  if [ -L "$d/target" ]; then
    continue
  elif [ -e "$d/target" ]; then
    echo "setup: $m/target is a real folder from before the workspace (delete it to link it)"
    continue
  fi
  # relative, so the link holds wherever the checkout lives
  up=$(echo "$m" | sed 's#[^/][^/]*#..#g')
  ln -s "$up/target" "$d/target"
done
