#!/bin/sh
# Goo for raea's physics (<Goo>, the `physics.goo` native call): objects melted into viscous goo by an MLS-MPM solver
# (Taichi on Metal, Apache-2.0), each frame's surface reconstructed (marching cubes, scikit-image: BSD) into a mesh
# sequence (.rseq) the engine plays back by frame; thin liquids ("solver": "sph") by SPlisHSPlasH (MIT) instead.
# run.py and sph.py are copied in beside the venv. Installs into $RAEA_GOO
# (default ~/.cache/raea-goo), outside the repo. raea runs this itself the first time a goo bake is asked for.
# pySPlisHSPlasH is pinned: 2.19.0 hangs after its first step on this Mac (2026-10-20); 2.17.0 runs.
# Size: about 600 MB (the venv: taichi, numpy, scipy, trimesh, scikit-image, pySPlisHSPlasH). No model weights.
#   tools/goo/setup.sh
set -e
DIR=${RAEA_GOO:-$HOME/.cache/raea-goo}
HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
if ! mkdir "$DIR/.setup-lock" 2>/dev/null; then
  echo "goo setup: another setup is running in $DIR (remove $DIR/.setup-lock if it isn't)" >&2
  exit 75
fi
trap 'rm -rf "$DIR/.setup-lock"' EXIT
trap 'exit 130' INT TERM
cd "$DIR"
say() { echo "goo setup: $*" >&2; }
rm -f .ready
# uv's own Python, not a framework build (Homebrew's): SPlisHSPlasH finds its emitters' meshes beside the running
# executable (the venv's bin/resources), and a framework Python runs as Python.app inside its own install
if [ -x .venv/bin/python ] && ! .venv/bin/python -c "import sys, sysconfig; sys.exit(1 if sysconfig.get_config_var('PYTHONFRAMEWORK') else 0)"; then
  say "remaking the venv on uv's Python (this one is a framework build)"
  rm -rf .venv
fi
if ! .venv/bin/python -c "import taichi, numpy, scipy, trimesh, skimage, zstandard, PIL, pysplishsplash" 2>/dev/null; then
  say "making the venv"
  [ -x .venv/bin/python ] || uv venv -q --python-preference only-managed --python 3.12 .venv
  VIRTUAL_ENV="$DIR/.venv" uv pip install -q --python .venv/bin/python taichi numpy scipy trimesh scikit-image zstandard pillow "pySPlisHSPlasH==2.17.0"
fi
.venv/bin/python -c "import taichi, numpy, trimesh, skimage; print('taichi', taichi.__version__, 'numpy', numpy.__version__, 'trimesh', trimesh.__version__, 'skimage', skimage.__version__)" >&2
cp "$HERE/run.py" "$HERE/sph.py" "$DIR/"
date -u +%Y-%m-%dT%H:%M:%SZ > "$DIR/.ready"
du -sh "$DIR" >&2
say "ready in $DIR"
