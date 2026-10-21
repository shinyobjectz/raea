#!/bin/sh
# Splat training for rdb's asset store: `Store::splat(input, {frames, steps, ...})` makes a 3D Gaussian splat from
# footage (frames, then COLMAP through pycolmap: BSD, then Brush: Apache-2.0, Rust + wgpu, trains on Metal) and keeps
# the .ply and its sidecar, cached by the input's content and the params. run.py is copied in beside the venv; Brush is
# its release binary (v0.3.0, aarch64-apple-darwin, checked against its SHA-256). Installs into $RAEA_SPLAT (default
# ~/.cache/raea-splat), outside the repo. rdb runs this itself the first time a splat is asked for; `raea-motion splat
# setup` runs it on purpose. Size: about 300 MB (the venv, Brush's 60 MB binary). No model weights.
#   tools/splat/setup.sh
set -e
DIR=${RAEA_SPLAT:-$HOME/.cache/raea-splat}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
if ! mkdir "$DIR/.setup-lock" 2>/dev/null; then
  echo "splat setup: another setup is running in $DIR (remove $DIR/.setup-lock if it isn't)" >&2
  exit 75
fi
trap 'rm -rf "$DIR/.setup-lock"' EXIT
trap 'exit 130' INT TERM
cd "$DIR"
say() { echo "splat setup: $*" >&2; }

# the venv: only what run.py imports, at matchmove's measured versions (2026-10-20), so uv's cache serves both
if ! .venv/bin/python -c "import pycolmap, numpy, cv2, PIL" 2>/dev/null; then
  say "making the venv"
  [ -x .venv/bin/python ] || uv venv -q --python 3.11 .venv
  VIRTUAL_ENV="$DIR/.venv" uv pip install -q --python .venv/bin/python "pycolmap==4.2.1" "numpy==2.4.6" "opencv-python-headless==5.0.0.93" "pillow==13.0.0"
fi

# Brush: the release binary for Apple silicon
BRUSH_VER=v0.3.0
BRUSH_SHA=65b2631398c839be3c1d4d7160fe2326389dec87830aac0710985e6690a1048c
if [ "$(uname -s)-$(uname -m)" != "Darwin-arm64" ]; then
  say "Brush's release binary here is for Apple silicon; elsewhere build it (cargo install --git https://github.com/ArthurBrussee/brush --tag $BRUSH_VER brush-app) and put brush_app in $DIR/brush/"
fi
if [ ! -x brush/brush_app ] || [ "$(cat brush/.version 2>/dev/null)" != "$BRUSH_VER" ]; then
  say "fetching Brush $BRUSH_VER"
  rm -rf brush.part && mkdir brush.part
  curl -fsSL "https://github.com/ArthurBrussee/brush/releases/download/$BRUSH_VER/brush-app-aarch64-apple-darwin.tar.xz" -o brush.part/brush.tar.xz
  echo "$BRUSH_SHA  brush.part/brush.tar.xz" | shasum -a 256 -c - >&2
  tar xJf brush.part/brush.tar.xz -C brush.part
  rm -rf brush && mkdir brush
  mv brush.part/brush-app-aarch64-apple-darwin/brush_app brush.part/brush-app-aarch64-apple-darwin/LICENSE brush/
  echo "$BRUSH_VER" > brush/.version
  rm -rf brush.part
fi
brush/brush_app --version >&2
cp "$HERE/run.py" "$DIR/run.py"
date -u +%Y-%m-%dT%H:%M:%SZ > "$DIR/.ready"
du -sh "$DIR" >&2
say "ready in $DIR"
