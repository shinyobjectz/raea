#!/bin/bash
# A release of the raea engine for macOS on Apple Silicon, Linux on x86_64 or Windows on x86_64 (whichever machine runs
# it; .github/workflows/release.yml builds all three on GitHub's runners for a tag, Windows in Git's bash): raea and
# its bundler, with raea's own files beside them (share/raea: the raea SDK's TypeScript, the lite runtime and its
# packages, the shell's sources, the tools that install the local music and foley models, and the agent skills), laid
# out as the installer puts it under ~/.raea. Built from the committed tree at REF (never anyone's unfinished edit);
# the runtime's packages (sdk/react/js/node_modules, installed, not committed) come from this checkout.
#
#   tools/release.sh VERSION BIN_DIR [REF]     BIN_DIR holds a release build's raea and its bundler
#                                              (libraea_bundle.dylib, .so, or raea.exe and raea_bundle.dll),
#                                              raea-perceive (`raea song` runs it; off Apple since #43), and the app
#                                              targets' two
#                                              hosts (docs/targets.feature T1f): raea-rune-host (native) and
#                                              raea-page-host's window build (web), each built in a cargo call of its
#                                              own, as they ship: `cargo build --profile dist -p raea-rune-host` and
#                                              `cargo build --profile dist -p raea-page-host --features window`
#
# The public engine is free and needs no raea account: its raea is built without raea's API (engine/cli's service
# feature) and with the plugin catalog (raea/plugins' 56 Bevy plugins and raea/input's gamepad and picking):
#   cargo build --profile dist --locked -p raea-cli --bin raea --no-default-features \
#     --features "$(bash tools/bevy-plugins.sh features sdk/raea/bench/bevy-plugins)"
# That build's raea-cli (thin LTO) prints nothing for over 25 minutes, so under antsy run it beside
# tools/cpu-heartbeat.sh (a line while rustc's CPU grows) and keep `--stall 25m`; a raised stall would only wait longer
# on a real hang.
#
# Writes dist/raea-VERSION-macos-arm64.tar.gz (-linux-x86_64.tar.gz, or -windows-x86_64.zip) and its .sha256, then
# checks the archive with tools/release-check.sh: unpacked into a fresh folder, its raea renders a frame with
# RAEA_NO_TREE=1 (so it can't be reading this checkout), has the plugin catalog, names none of raea's API and holds
# nothing key-shaped (on Windows, no Microsoft file). Nothing is published.
set -eu
version=${1:?usage: release.sh VERSION BIN_DIR [REF]}
bin=$(cd "${2:?usage: release.sh VERSION BIN_DIR [REF]}" && pwd)
ref=${3:-HEAD}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out=$root/dist
case $(uname -s) in
  Linux) platform=linux-x86_64 ;;
  MINGW*|MSYS*|CYGWIN*) platform=windows-x86_64 ;;
  *) platform=macos-arm64 ;;
esac
name=raea-$version-$platform
stage=$(mktemp -d)/raea
mkdir -p "$stage/bin" "$stage/share/raea" "$out"
# the skills gate (docs/skills.feature SK9): the committed tree at REF against this release's raea, so a stale reference,
# a retired word, a flag no verb takes or a dead link refuses the release
gate=$(mktemp -d)
# Windows makes no symlinks: tar fails on each of the tree's (sdk/*/node_modules, old content links), which the skills
# check never reads. There, those failures alone are left out and counted; any other is the release's (T1f, 11-07)
untar() {
  local err rc=0
  err=$(tar -x -C "$1" 2>&1 >/dev/null) || rc=$?
  [ $rc -eq 0 ] && return 0
  if [ "$platform" = windows-x86_64 ] && ! grep -v -e "Cannot create symlink" -e "Exiting with failure status" <<< "$err" | grep -q .; then
    echo "release.sh: note: the skills gate left out $(grep -c "Cannot create symlink" <<< "$err") symlinks (Windows makes none)"
    return 0
  fi
  printf '%s\n' "$err" >&2
  return $rc
}
raea_bin=$bin/raea; [ -f "$raea_bin.exe" ] && raea_bin=$raea_bin.exe
# The gate reads the feed app's checkout (apps/app), which is private: a public checkout (release.yml's runners) has
# none. There, RAEA_SKILLS_PASSED=SHA stands for it: the gate passed at SHA where the feed app is (a maintainer's
# checkout), and it counts for REF only when what the gate reads (the skills and guides, the glossary, the CLI's usage,
# skills.sh) is the same at both. With neither, the release refuses (T1f, 11-07)
# the raea skill lives in the raea-skills submodule (skills/), so its pointer stands for their text
gate_inputs=(skills sdk/guides docs/skills.glossary.md engine/cli/src tools/skills.sh)
# raea-skills (skills/) at the commit REF pins, written into DIR: a submodule isn't in REF's own archive
skills_at() {
  local pin; pin=$(git rev-parse "$ref:skills") || { echo "release.sh: $ref pins no raea-skills (skills/)" >&2; return 1; }
  git -C skills cat-file -e "$pin^{commit}" 2>/dev/null || git -C skills fetch -q origin "$pin" || { echo "release.sh: raea-skills has no commit $pin (git submodule update --init skills)" >&2; return 1; }
  mkdir -p "$1" && git -C skills archive "$pin" | tar -x -C "$1"
}
if [ ! -f "$root/apps/app/src/acp/protocol.ts" ]; then
  [ -n "${RAEA_SKILLS_PASSED:-}" ] || { echo "release.sh: no feed app here (apps/app), so the skills gate can't run; run tools/skills.sh check in a checkout with the feed app, then name the commit it passed on as RAEA_SKILLS_PASSED (release.yml: a \"skills-passed: <sha>\" line in the annotated tag's message, or the skills_passed input when run by hand)" >&2; exit 1; }
  git rev-parse -q --verify "$RAEA_SKILLS_PASSED^{commit}" >/dev/null || { echo "release.sh: RAEA_SKILLS_PASSED=$RAEA_SKILLS_PASSED isn't a commit here (a full SHA, fetched)" >&2; exit 1; }
  git diff --quiet "$RAEA_SKILLS_PASSED" "$ref" -- "${gate_inputs[@]}" \
    || { echo "release.sh: the skills changed between $RAEA_SKILLS_PASSED (where the gate passed) and $ref" >&2; exit 1; }
  echo "release.sh: note: the skills gate passed at $(git rev-parse --short "$RAEA_SKILLS_PASSED") elsewhere and its inputs are unchanged; not run here (no feed app)"
  # but cli.md is checked here, against this release's own build, which no other checkout has
  RAEA_BIN="$(cd "$(dirname "$raea_bin")" && pwd)/$(basename "$raea_bin")" bash tools/skills.sh check-cli || { echo "release.sh: cli.md doesn't match $raea_bin's help" >&2; exit 1; }
else
  git archive "$ref" | untar "$gate" || { echo "release.sh: the skills gate couldn't unpack $ref" >&2; exit 1; }
  skills_at "$gate/skills" || exit 1
  git -C "$gate" init -q && git -C "$gate" add -A && git -C "$gate" -c gc.auto=0 -c maintenance.auto=false -c user.name=release -c user.email=release@raea.io commit -qm "$ref" --no-verify
  if ! (cd "$gate" && SKILLS_STRICT=1 SKILLS_CHANGED= RAEA_BIN="$raea_bin" RAEA_APP="$root/apps/app" RAEA_APP_REV="$(git -C "$root" rev-parse "$ref:apps/app")" bash tools/skills.sh check); then
    echo "release.sh: tools/skills.sh check failed on $ref with $raea_bin; fix the skills (tools/skills.sh gen) before releasing" >&2
    rm -rf "$gate"
    exit 1
  fi
fi
rm -rf "$gate"
# the skills' changes since the last release, for its notes (SK9): RAEA_SINCE names where that was (a ref, or a date
# like 2026-10-31), else the last v* tag; written beside the archive as raea-VERSION-skills.md
since=${RAEA_SINCE:-$(git describe --tags --abbrev=0 --match 'v*' "$ref" 2>/dev/null || true)}
{
  echo "## The agent skills and guides, since ${since:-the start}"
  echo
  if [ -z "$since" ]; then range=("$ref"); elif git rev-parse -q --verify "$since^{commit}" >/dev/null; then range=("$since..$ref"); else range=(--since="$since" "$ref"); fi
  git log --no-merges --format='- %s' "${range[@]}" -- sdk/skills sdk/guides ':!sdk/skills/raea-work' | cut -c1-200
  # and raea-skills' own, between the commit the last release pinned (none before the move) and this one
  from=$( [ -n "$since" ] && git rev-parse -q --verify "$since:skills" 2>/dev/null || true )
  git -C skills log --no-merges --format='- %s' ${from:+"$from.."}"$(git rev-parse "$ref:skills")" -- skills | cut -c1-200
} > "$out/raea-$version-skills.md"
case $platform in
  linux-x86_64)
    cp "$bin/raea" "$bin/libraea_bundle.so" "$bin/raea-perceive" "$bin/raea-rune-host" "$bin/raea-page-host" "$stage/bin/"
    # what a Linux machine needs beside it: the bundled Noto Sans JP is a Latin subset, so kanji and kana come from the
    # system, as on a Mac (raea warns once a character when no font has it)
    printf '%s\n' "raea $version for Linux x86_64 (glibc 2.39 or newer, Vulkan: a GPU driver or Mesa's llvmpipe).
Install a CJK font for Japanese, Chinese and Korean text (Debian and Ubuntu: apt install fonts-noto-cjk).
An app's window, on either target (raea app run, raea-rune-host and raea-page-host), needs a display (X11 or
Wayland; Xvfb on a server, as raea app check does there too), libxkbcommon-x11, the X client libraries and the Vulkan
loader (Debian and Ubuntu: apt install libxkbcommon-x11-0 libxcursor1 libxi6 libxrandr2 libx11-xcb1 libvulkan1)." > "$stage/README" ;;
  windows-x86_64)
    cp "$bin/raea.exe" "$bin/raea_bundle.dll" "$bin/raea-perceive.exe" "$bin/raea-rune-host.exe" "$bin/raea-page-host.exe" "$stage/bin/"
    # the C runtime isn't in the zip (ort's ONNX Runtime needs its DLLs, and they're Microsoft's to ship): the VC++
    # redistributable, which most Windows machines already have, provides it
    printf '%s\r\n' "raea $version for Windows x86_64 (Windows 10 or 11, a DirectX 12 or Vulkan GPU driver; with no GPU, Mesa's lavapipe and WGPU_BACKEND=vulkan)." \
      "It needs the Microsoft Visual C++ Redistributable (x64), which most machines already have:" \
      "https://aka.ms/vs/17/release/vc_redist.x64.exe" > "$stage/README.txt" ;;
  *) cp "$bin/raea" "$bin/libraea_bundle.dylib" "$bin/raea-perceive" "$bin/raea-rune-host" "$bin/raea-page-host" "$stage/bin/" ;;
esac
# the packages sdk/tsconfig.base.json lists (@raea/<name>: its sources and package.json), which the bundlers resolve by it
pkgs=()
# and its bevy/ (a package's plugin handles, generated from the plugin build: its src/plugin.ts imports ../bevy/)
# and its examples/ (@raea/rune's, docs/targets.feature T1b: what a release's check runs)
for p in $(git show "$ref:sdk/tsconfig.base.json" | jq -r '.compilerOptions.paths | to_entries[] | select(.key | test("^@raea/[a-z0-9_]+$")) | .value[0] | ltrimstr("./") | split("/")[0]' | tr -d '\r'); do
  pkgs+=("sdk/$p/src" "sdk/$p/package.json")
  git cat-file -e "$ref:sdk/$p/bevy" 2>/dev/null && pkgs+=("sdk/$p/bevy")
  git cat-file -e "$ref:sdk/$p/examples" 2>/dev/null && pkgs+=("sdk/$p/examples")
done
[ ${#pkgs[@]} -gt 0 ] || { echo "release: no @raea packages in sdk/tsconfig.base.json at $ref" >&2; exit 1; }
# the committed files raea reads at run time
# (@raea/ui, the page's DOM every render goes through, isn't an @raea/<name> path, so it's named here)
git archive "$ref" sdk/tsconfig.base.json sdk/ui/src sdk/ui/package.json sdk/ui/bevy sdk/raea/src sdk/raea/library sdk/raea/package.json sdk/react/js/src sdk/react/js/lite sdk/react/js/shims sdk/react/js/package.json sdk/react/js/tsconfig.json sdk/shell/src \
  "${pkgs[@]}" \
  tools/tokensynth tools/sao tools/goo tools/splat tools/matchmove tools/ensure.sh sdk/guides/craft docs/sdk-coverage.md engine/cli/template/theme.css ':(glob)engine/cli/src/*.ts' \
  | tar -x -C "$stage/share/raea"
# a folder copied as it is here (links kept, as rsync -a; tar, since Git's bash on Windows has no rsync)
copy() { mkdir -p "$2" && tar -C "$(dirname "$1")" --exclude .cache -cf - "$(basename "$1")" | tar -C "$2" -xf -; }
# styles.gen's Tailwind (raea compiles it on JavaScriptCore: runtime/bundle/src/styles.rs), as installed here
copy sdk/react/tw/node_modules/tailwindcss "$stage/share/raea/sdk/react/tw/node_modules"
# `raea song`'s beat model (Beat This! small0, MIT), copied on first use where raea-perceive reads it
# (raea_io_home::models: ~/.local/share/raea-models, %LOCALAPPDATA%\raea\models on Windows); every platform since #43
mkdir -p "$stage/share/raea/models/beat-this"
cp ~/.local/share/raea-models/beat-this/beat_this_small0.onnx "$stage/share/raea/models/beat-this/"
# the agent skill (raea: raea-skills at the commit REF pins), which the installer copies into
# ~/.claude/skills; raea-work (sdk/skills, work.raea.io's showcase pages) is internal and stays out, and _template is
# raea-skills' start of a new skill
skills=$(mktemp -d)
skills_at "$skills" || exit 1
rm -rf "$skills/skills/_template"
cp -R "$skills/skills" "$stage/share/raea/skills"
# the runtime's packages, as installed here
copy sdk/react/js/node_modules "$stage/share/raea/sdk/react/js"
rm -rf "$stage/share/raea/tools/tokensynth/__pycache__" "$skills"
printf '%s\n' "$version" > "$stage/share/raea/VERSION"
# each skill's front matter names the raea it shipped with, so an agent holding an older or newer copy than its raea
# can tell, and reads `raea skill` instead (docs/skills.feature SK6; `raea skills install` stamps the same way)
for md in "$stage"/share/raea/skills/*/SKILL.md; do
  perl -0pi -e 's/\A---\n(.*?)\n---\n/"---\n" . join("\n", grep { !\/^version:\/ } split(\/\n\/, $1)) . "\nversion: '"$version"'\n---\n"/se' "$md"
done
if [ $platform = windows-x86_64 ]; then
  archive=$name.zip
  rm -f "$out/$archive"
  (cd "$(dirname "$stage")" && 7z a -tzip -bso0 -bsp0 "$out/$archive" raea)
else
  archive=$name.tar.gz
  tar -C "$(dirname "$stage")" -czf "$out/$archive" raea
fi
(cd "$out" && if command -v sha256sum >/dev/null; then sha256sum "$archive"; else shasum -a 256 "$archive"; fi > "$archive.sha256")
rm -rf "$(dirname "$stage")"
bash "$root/tools/release-check.sh" "$out/$archive"
echo "release: $out/$archive ($(du -h "$out/$archive" | cut -f1))"
