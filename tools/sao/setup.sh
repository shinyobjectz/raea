#!/bin/sh
# Stable Audio Open Small on Core ML (stabilityai/stable-audio-open-small, Stability AI Community License; the Core ML
# packages are mlboydaisuke/Stable-Audio-Open-Small-CoreML) for rdb's asset store: `make("sao", prompt, seed, {seconds})`
# renders one sound locally (run.py, copied in beside the models). Installs into $RAEA_SAO (default ~/.cache/raea-sao),
# outside the repo, as TokenSynth's lives in ~/.cache/raea-tokensynth. rdb runs this itself the first time a sound is
# asked for and nothing is installed; `raea-motion setup-audio` runs it on purpose.
#
# What it fetches, once: the DiT (1.36 GB), the VAE decoder (156 MB) and the number embedder (0.4 MB) at a pinned
# revision, each checked against its SHA-256; t5-base's tokenizer.json (1.4 MB), so runs never touch the network; and
# a small venv (numpy, coremltools, tokenizers: ~95 MB). The published fp16 T5 encoder overflows, so the T5 is raea's
# own fp32 conversion (t5_to_coreml.py, 419 MB): made here once in a throwaway environment with torch and t5-base's
# weights (892 MB, deleted after), unless another install has it to copy.
#
# $RAEA_SAO_FROM names an existing install to copy models from instead of downloading them (default ~/.cache/raea-sao
# when installing somewhere else). Copies are APFS clones (cp -c): no extra disk until one side changes.
#   tools/sao/setup.sh
set -e
DIR=${RAEA_SAO:-$HOME/.cache/raea-sao}
HERE=$(cd "$(dirname "$0")" && pwd)
FROM=${RAEA_SAO_FROM:-$HOME/.cache/raea-sao}
REPO=mlboydaisuke/Stable-Audio-Open-Small-CoreML
REV=8fa80903adebfeb62fc885c45505ac81628f3320
T5REV=a9723ea7f1b39c1eae772870f3b547bf6ef7e6c1
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
[ "$(cd "$FROM" 2>/dev/null && pwd)" = "$DIR" ] && FROM=
# one setup at a time: a render and a CLI asking for the first sound together would both start one
if ! mkdir "$DIR/.setup-lock" 2>/dev/null; then
  echo "sao setup: another setup is running in $DIR (remove $DIR/.setup-lock if it isn't)" >&2
  exit 75
fi
trap 'rm -rf "$DIR/.setup-lock"' EXIT
trap 'exit 130' INT TERM
cd "$DIR"
say() { echo "sao setup: $*" >&2; }

# a file of the installation: kept if it's here, cloned from $FROM if that has it, else downloaded; checked against
# its SHA-256 when one is given. fetch DEST URL [SHA256]
fetch() {
  dest=$1 url=$2 sum=$3
  [ -s "$dest" ] && return 0
  mkdir -p "$(dirname "$dest")"
  if [ -n "$FROM" ] && [ -s "$FROM/$dest" ]; then
    cp -c "$FROM/$dest" "$dest.part" 2>/dev/null || cp "$FROM/$dest" "$dest.part"
  else
    say "downloading $dest"
    curl -fL --retry 3 --progress-bar -o "$dest.part" "$url"
  fi
  if [ -n "$sum" ] && [ "$(shasum -a 256 "$dest.part" | cut -d' ' -f1)" != "$sum" ]; then
    rm -f "$dest.part"
    say "$dest doesn't match its SHA-256"
    exit 1
  fi
  mv -f "$dest.part" "$dest"
}

# the venv: only what run.py imports. An existing one that already has them is left alone
if ! .venv/bin/python -c "import numpy, coremltools, tokenizers" 2>/dev/null; then
  say "making the venv"
  [ -x .venv/bin/python ] || uv venv -q --python 3.11 .venv
  VIRTUAL_ENV="$DIR/.venv" uv pip install -q --python .venv/bin/python "numpy==2.4.6" "coremltools==9.0" "tokenizers==0.23.3"
fi

HF=https://huggingface.co/$REPO/resolve/$REV
P=Data/com.apple.CoreML
while read -r name mlmodel weights; do
  fetch "coreml-ref/StableAudio$name.mlpackage/Manifest.json" "$HF/StableAudio$name.mlpackage/Manifest.json"
  fetch "coreml-ref/StableAudio$name.mlpackage/$P/model.mlmodel" "$HF/StableAudio$name.mlpackage/$P/model.mlmodel" "$mlmodel"
  fetch "coreml-ref/StableAudio$name.mlpackage/$P/weights/weight.bin" "$HF/StableAudio$name.mlpackage/$P/weights/weight.bin" "$weights"
done <<EOF
NumberEmbedder 14ecb9a0f6c3d04c3a611352343d7dea0c5b0720c9f8b56d87b8eae794b9c64c 8ae6ab6d7bf960678564163be68ad300e245030b56eb5db10b2ab3e9090220d8
VAEDecoder 9614b1ec8ba90995c1b8e797da19ba21fc4acc67b0dfe0bbb28527dcbfc0a262 a8a1bdb84b854f38e2ef1d3adb5860a03303a442f8eaa8d567aeb606e66a4fc8
DiT a118772c17fb831a6eda6a1ec2527f535d004064e27a8eb4597d5ae6d33a0576 c5444973f1a2bb3878a33049a4c559a1097de77fca8dc358aba38ee72033e97d
EOF

# the tokenizer from the install, never the Hugging Face cache (the spike's runner read ~/.cache/huggingface)
fetch t5-tokenizer/tokenizer.json "https://huggingface.co/google-t5/t5-base/resolve/$T5REV/tokenizer.json" \
  d2acde0d8d71dd30a711834b07781b9c89feaac33fd332f60507699282740066

T5=coreml/T5EncoderFP32.mlpackage
# made under another name until it's whole (coremltools insists on the .mlpackage extension)
PART=coreml/T5EncoderFP32.part.mlpackage
if [ ! -s "$T5/Manifest.json" ]; then
  mkdir -p coreml
  rm -rf "$PART"
  if [ -n "$FROM" ] && [ -s "$FROM/$T5/Manifest.json" ]; then
    cp -cR "$FROM/$T5" "$PART" 2>/dev/null || cp -R "$FROM/$T5" "$PART"
  else
    # torch, transformers and t5-base's weights only for this one conversion, in uv's throwaway environment and a
    # Hugging Face cache that's deleted after (on failure too, by the trap): none of it stays in the install
    say "converting t5-base's encoder to Core ML at fp32 (a one-time ~1 GB download, deleted after)"
    trap 'rm -rf "$DIR/.setup-lock" "$DIR/.t5-hf"' EXIT
    HF_HOME="$DIR/.t5-hf" nice -n 15 uv run -q --no-project --python 3.11 --with "torch==2.7.1" --with "transformers==5.19.0" \
      --with "coremltools==9.0" --with "numpy==2.4.6" python -I "$HERE/t5_to_coreml.py" --revision "$T5REV" \
      --tokenizer t5-tokenizer/tokenizer.json --out "$PART"
    rm -rf .t5-hf
  fi
  mv "$PART" "$T5"
fi

cp "$HERE/run.py" "$DIR/run.py"
date -u +%Y-%m-%dT%H:%M:%SZ > "$DIR/.ready"
du -sh "$DIR/coreml-ref" "$DIR/coreml" "$DIR/.venv" >&2
say "ready in $DIR"
