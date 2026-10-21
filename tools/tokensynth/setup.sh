#!/bin/sh
# TokenSynth (MIT code and weights: github.com/KyungsuKim42/tokensynth, huggingface.co/KyungsuKim/TokenSynth) for rdb's
# asset store: `make("tokensynth", timbre, seed, {midi, velocity, seconds, precision})` renders one note (torch on the CPU:
# MPS is 6x slower and can't run DAC's decoder on long clips), cached as PCM by everything that decides the bytes.
# convert.py then makes the smaller runtime run.py defaults to (weights-fp16/, 1.0 GB) and the Core ML models (coreml/).
# Installs into $RAEA_TOKENSYNTH (default ~/.cache/raea-tokensynth), outside the repo, as SAO's lives in ~/.cache/raea-sao.
# rdb runs this itself the first time a note is asked for and nothing is installed; `raea-motion setup-audio` runs it on
# purpose.
# Size: the model 713 MB, its unconditional twin 713 MB (text timbres need it), DAC 307 MB, CLAP 2.35 GB, three text
# encoders laion-clap fetches (479 MB), and the venv (1.1 GB). Local only: no API model is called.
#
# $RAEA_TOKENSYNTH_FROM names an existing install to copy the weights and the converted runtime from instead of
# downloading and converting them (default ~/.cache/raea-tokensynth when installing somewhere else). Copies are APFS
# clones (cp -c): no extra disk until one side changes.
#   tools/tokensynth/setup.sh
set -e
DIR=${RAEA_TOKENSYNTH:-$HOME/.cache/raea-tokensynth}
HERE=$(cd "$(dirname "$0")" && pwd)
FROM=${RAEA_TOKENSYNTH_FROM:-$HOME/.cache/raea-tokensynth}
mkdir -p "$DIR/weights" "$DIR/hf"
DIR=$(cd "$DIR" && pwd)
[ "$(cd "$FROM" 2>/dev/null && pwd)" = "$DIR" ] && FROM=
# one setup at a time: a render and a CLI asking for the first note together would both start one
if ! mkdir "$DIR/.setup-lock" 2>/dev/null; then
  echo "tokensynth setup: another setup is running in $DIR (remove $DIR/.setup-lock if it isn't)" >&2
  exit 75
fi
trap 'rm -rf "$DIR/.setup-lock"' EXIT
trap 'exit 130' INT TERM
cd "$DIR"
say() { echo "tokensynth setup: $*" >&2; }
clone() { cp -cR "$1" "$2" 2>/dev/null || cp -R "$1" "$2"; }

if [ ! -d repo ]; then
  if [ -n "$FROM" ] && [ -d "$FROM/repo" ]; then clone "$FROM/repo" repo; else git clone -q --depth 1 https://github.com/KyungsuKim42/tokensynth repo; fi
fi
git -C repo rev-parse HEAD > repo.commit
# the versions the spike ran (2026-10-18): laion-clap 1.1.6 needs transformers 4.30; scipy 1.15's wheel won't load here
# coremltools for convert.py's Core ML models (run.py --precision coreml). An existing venv that has them is left alone
if ! .venv/bin/python -c "import torch, laion_clap, dac, tokensynth, coremltools" 2>/dev/null; then
  say "making the venv (about 1.1 GB)"
  [ -x .venv/bin/python ] || uv venv -q --python 3.11 .venv
  VIRTUAL_ENV="$DIR/.venv" uv pip install -q --python .venv/bin/python "torch==2.5.1" "torchaudio==2.5.1" "transformers==4.30.2" "scipy==1.14.1" "coremltools==8.3.0" laion-clap descript-audio-codec pretty_midi soundfile numpy -e repo
fi
for f in token_synth_aug.pt token_synth_unconditional.pt dac_weights_44khz_8kbps_0.0.1.pt clap_music_audioset_epoch_15_esc_90.14.pt; do
  if [ ! -s "weights/$f" ]; then
    if [ -n "$FROM" ] && [ -s "$FROM/weights/$f" ]; then
      clone "$FROM/weights/$f" "weights/$f.part"
    else
      say "downloading weights/$f"
      curl -fL --retry 3 --progress-bar -o "weights/$f.part" "https://huggingface.co/KyungsuKim/TokenSynth/resolve/main/$f"
    fi
    mv -f "weights/$f.part" "weights/$f"
  fi
done
# the converted runtime from the other install, when it has it whole: convert.py then finds everything made
for d in weights-fp16 coreml; do
  if [ ! -e "$d" ] && [ -n "$FROM" ] && [ -d "$FROM/$d" ]; then clone "$FROM/$d" "$d"; fi
done
cp "$HERE/run.py" "$DIR/run.py"
# the smaller runtime run.py's other precisions read (weights-fp16/, coreml/); keeps what is already made
nice -n 15 .venv/bin/python -I "$HERE/convert.py" --dir "$DIR"
date -u +%Y-%m-%dT%H:%M:%SZ > "$DIR/.ready"
du -sh "$DIR/weights" "$DIR/weights-fp16" "$DIR/coreml" "$DIR/.venv" >&2
say "ready in $DIR (repo $(cat repo.commit))"
