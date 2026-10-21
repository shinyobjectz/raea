#!/bin/sh
# Matchmove (camera solve) for rdb's asset store: `Store::matchmove(clip, {from, seconds, subject, height})` solves the
# camera of a stretch of footage (COLMAP through pycolmap: BSD; OpenCV for the rotation-only fallback: Apache-2.0) and
# keeps the solve as JSON, cached by the clip's content and the params. run.py is copied in beside the venv. Installs
# into $RAEA_MATCHMOVE (default ~/.cache/raea-matchmove), outside the repo, as SAO's lives in ~/.cache/raea-sao. rdb runs
# this itself the first time a solve is asked for and nothing is installed; `raea-motion track --setup` runs it on
# purpose. Size: the venv only (pycolmap, numpy, opencv-python-headless, pillow: about 250 MB). No model weights.
#   tools/matchmove/setup.sh
set -e
DIR=${RAEA_MATCHMOVE:-$HOME/.cache/raea-matchmove}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
# one setup at a time: a render and a CLI asking for the first solve together would both start one
if ! mkdir "$DIR/.setup-lock" 2>/dev/null; then
  echo "matchmove setup: another setup is running in $DIR (remove $DIR/.setup-lock if it isn't)" >&2
  exit 75
fi
trap 'rm -rf "$DIR/.setup-lock"' EXIT
trap 'exit 130' INT TERM
cd "$DIR"
say() { echo "matchmove setup: $*" >&2; }

# the venv: only what run.py imports, at the versions measured (2026-10-20). An existing one that has them is left alone
if ! .venv/bin/python -c "import pycolmap, numpy, cv2, PIL" 2>/dev/null; then
  say "making the venv"
  [ -x .venv/bin/python ] || uv venv -q --python 3.11 .venv
  VIRTUAL_ENV="$DIR/.venv" uv pip install -q --python .venv/bin/python "pycolmap==4.2.1" "numpy==2.4.6" "opencv-python-headless==5.0.0.93" "pillow==13.0.0"
fi
cp "$HERE/run.py" "$DIR/run.py"
date -u +%Y-%m-%dT%H:%M:%SZ > "$DIR/.ready"
du -sh "$DIR/.venv" >&2
say "ready in $DIR"
