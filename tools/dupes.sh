#!/bin/bash
# Copied code, as a ratchet: exits 1 when cargo-dupes finds more exact or near-duplicate groups in raea's own Rust
# than tools/dupes.max allows. When it finds fewer, it says so: lower the numbers in the same commit, so they only
# go down. Vendored crates, the conformance runner (sdk/ui/conformance: a test tool in its own workspace, its
# one-line closures matching other crates') and tests are left out: #[cfg(test)] modules and files named tests.rs (a
# module's tests split into a file of their own, which cargo-dupes reads as code). So is engine/bevy-plugins/src/
# declared.rs, which tools/bevy-plugins.sh generates: one registration closure per catalog plugin, alike by design.
#   tools/dupes.sh            the check (L0, tools/agents/check)
#   tools/dupes.sh report     every group, to find what to fold together
# Needs cargo-dupes (cargo install cargo-dupes --locked).
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2
# tools/rask and skills (raea-skills) are their own repositories (submodules, empty in a fresh export), so they never count
X=(--exclude vendor --exclude target --exclude sdk/ui/conformance --exclude-tests --exclude tests.rs --exclude engine/bevy-plugins/src/declared.rs \
   --exclude tools/rask --exclude skills)
command -v cargo-dupes >/dev/null || { echo "dupes: cargo-dupes isn't installed (cargo install cargo-dupes --locked)"; exit 1; }
[ "${1:-}" = report ] && exec cargo dupes report "${X[@]}"

read -r max_exact max_near < <(awk -F= '/^exact=/{e=$2} /^near=/{n=$2} END{print e, n}' tools/dupes.max)
[ -n "$max_exact" ] && [ -n "$max_near" ] || { echo "dupes: tools/dupes.max names no exact= and near= limits"; exit 1; }
out=$(cargo dupes stats "${X[@]}" 2>&1) || { echo "$out"; exit 1; }
exact=$(echo "$out" | awk '/^Exact duplicates:/{print $3}')
near=$(echo "$out" | awk '/^Near duplicates:/{print $3}')
[ -n "$exact" ] && [ -n "$near" ] || { echo "dupes: couldn't read cargo-dupes' stats:"; echo "$out"; exit 1; }
if [ "$exact" -gt "$max_exact" ] || [ "$near" -gt "$max_near" ]; then
  echo "dupes: $exact exact and $near near-duplicate groups (allowed $max_exact and $max_near): fold the new copy into"
  echo "  what it copies (tools/dupes.sh report lists every group)"
  exit 1
fi
if [ "$exact" -lt "$max_exact" ] || [ "$near" -lt "$max_near" ]; then
  echo "dupes: ok, and down to $exact exact and $near near: set tools/dupes.max to exact=$exact near=$near"
else
  echo "dupes: ok ($exact exact, $near near)"
fi
