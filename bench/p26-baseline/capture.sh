#!/bin/bash
# P26's baseline (docs/ts-native.feature P26a): frames of every composition in list.txt rendered by the published raea
# 0.1.9, the reference every later page must match (SSIM >= 0.98 per frame, fonts and brand styles included). It was
# the last Blitz build's until that build and its frames were lost (after 11-16); re-taken from 0.1.9 on 11-22.
#
#   bench/p26-baseline/capture.sh [RAEA_BIN]      (default: 0.1.9's raea, unpacked at
#                                                  ~/Library/Caches/raea/p26-baseline/bin-v0.1.9, P26_STORE to move it)
#
# For each composition: its length probed (getCompositions, through `raea run`), nine frames rendered with
# --manifest (0 to the last, evenly), and four kept: the first, the middle, the last and the one with the most text in
# its manifest (a composition whose length couldn't be probed is taken as 60 frames: probed false). Kept under $P26_STORE/frames/<slug>/<frame>.png (not in git: the frames are 89 MB; each one's sha256 is in the manifest), each listed in
# manifest.jsonl with the composition, frame, why it was kept, the PNG's sha256 and the binary's. Failures go to failures.txt.
# ONLY=substring limits the list (LIST=file takes the compositions from a file instead): the other compositions'
# entries in manifest.jsonl and failures.txt are kept and the listed ones replaced. Runs one render at a time (the GPU
# is shared). A composition's props, if it takes any, come from props/<slug>/wrap.tsx, a wrapper giving it its documented example props.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
. "$HERE/lib.sh"
ROOT=$(cd "$HERE/../.." && pwd)
# the baseline's home outside the checkout and /tmp: 0.1.9's raea under bin-v0.1.9/ (its release tarball, unpacked),
# the frames under frames/
STORE=${P26_STORE:-$HOME/Library/Caches/raea/p26-baseline}
BIN=${1:-$STORE/bin-v0.1.9/raea/bin/raea}
[ -x "$BIN" ] || { echo "capture.sh: no raea at $BIN" >&2; exit 2; }
SHA=$(shasum -a 256 "$BIN" | cut -d' ' -f1)
BIN_SHOWN="~${BIN#"$HOME"}"; [ "${BIN#"$HOME"}" = "$BIN" ] && BIN_SHOWN=$BIN
OUT=$STORE/frames
WORK=$(mktemp -d "${TMPDIR:-/tmp}/p26-baseline.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"
LIST=${LIST:-$HERE/list.txt}
touch "$HERE/manifest.jsonl" "$HERE/failures.txt"
# the compositions this run takes, and the earlier entries kept (everything else's)
picked=$(grep -v '^$' "$LIST" | { [ -n "${ONLY:-}" ] && grep -F -- "$ONLY" || cat; })
if [ -z "${ONLY:-}" ] && [ "$LIST" = "$HERE/list.txt" ]; then
  : > "$HERE/manifest.jsonl"; : > "$HERE/failures.txt"
else
  for f in manifest.jsonl failures.txt; do
    while read -r rel; do
      [ -n "$rel" ] || continue
      rel=${rel%%  #*}; rel=${rel%%  @*}
      grep -v -F -- "\"composition\":\"$rel\"" "$HERE/$f" | grep -v -F -- "$rel:" > "$WORK/keep" || true
      cat "$WORK/keep" > "$HERE/$f"
    done <<< "$picked"
  done
  while read -r rel; do rel=${rel%%  #*}; rel=${rel%%  @*}; slug=$(echo "$rel" | sed 's#/#__#g; s#\.tsx#@#; s#@$##; s#@\##@#'); [ -n "$rel" ] && rm -rf "$OUT/$slug"; done <<< "$picked"
fi
[ -f "$HERE/binary.json" ] && [ -n "${ONLY:-}${LIST#$HERE/list.txt}" ] || echo "{\"binary\":\"$BIN_SHOWN\",\"sha256\":\"$SHA\",\"dylib_sha256\":\"$(shasum -a 256 "$(dirname "$BIN")/libraea_bundle.dylib" 2>/dev/null | cut -d' ' -f1)\",\"head\":\"$(git -C "$ROOT" rev-parse --short HEAD)\",\"date\":\"$(date -u +%FT%TZ)\"}" > "$HERE/binary.json"
n=0
while read -r rel; do
  [ -z "$rel" ] && continue
  # list.txt's "path  # excluded: why": listed in failures.txt with its reason, not rendered
  if [[ "$rel" == *"# excluded:"* ]]; then
    echo "${rel%%  #*}: EXCLUDED  ${rel##*# excluded: }" >> "$HERE/failures.txt"
    echo "skip ${rel%%  #*}"
    continue
  fi
  # "path  @A,B,C": frames to keep besides the four (a screen's picture, a cut's busiest moment)
  asked=""
  if [[ "$rel" == *"  @"* ]]; then asked=${rel##*  @}; rel=${rel%%  @*}; fi
  p26_src "$rel"
  # its length: the first registered composition's, else its `config`, else the CLI's default (60 frames)
  cat > "$WORK/probe.ts" <<EOF
import * as M from "$src";
import { getCompositions } from "raea";
import { native } from "raea/native";
export default function () {
  const c: any = getCompositions().find((x: any) => "$comp" === "" || x.id === "$comp") ?? null;
  const cfg: any = (M as any).config ?? {};
  native("print" as never, { text: "P26PROBE " + JSON.stringify({ id: c?.id ?? null, frames: c?.durationInFrames ?? cfg.durationInFrames ?? 60 }) } as never);
}
EOF
  probe=$(cd "$dir" && "$BIN" run "$WORK/probe.ts" 2>&1 | grep -o 'P26PROBE .*' | head -1 | cut -d' ' -f2-)
  total=$(echo "$probe" | jq -r '.frames // 60' 2>/dev/null)
  # unprobed: the script bundler couldn't take it (a subpath only the render bundler has); its length is the default
  probed=$([ -n "$probe" ] && echo true || echo false)
  [[ "$total" =~ ^[0-9]+$ ]] || total=60
  [ "$total" -lt 1 ] && total=1
  last=$((total - 1))
  cands=$({ for i in 0 1 2 3 4 5 6 7 8; do echo $((last * i / 8)); done; echo "$asked" | tr ',' '\n' | grep . | awk -v l=$last '$1 <= l'; } | sort -n | awk '!seen[$0]++' | paste -sd, -)
  rm -rf "$WORK/r" && mkdir -p "$WORK/r"
  # the checks let through, as compare.sh does: a row is its pixels, and a scene that clips is still a frame to match
  if ! (cd "$dir" && RAEA_DESPITE="all: the P26 baseline keeps a row's pixels; what its scene clips is in the log" "$BIN" render "$src" ${copt[@]+"${copt[@]}"} --at "$cands" -o "$WORK/r/" --manifest "$WORK/r/m.json") > "$WORK/log" 2>&1; then
    echo "$rel: $(grep -E 'REFUSED|error|Error' "$WORK/log" | head -2 | tr '\n' ' ')" >> "$HERE/failures.txt"
    echo "MISS $rel"
    continue
  fi
  # the frame with the most text (its manifest's words), among the candidates
  texty=$(jq -r '[.frames[] | {f: .frame, n: ([.text[]?.text | length] | add // 0)}] | max_by(.n) | .f' "$WORK/r/m.json" 2>/dev/null)
  mid=$(echo "$cands" | tr ',' '\n' | sed -n 5p)
  IFS=, read -ra list <<< "$cands"
  mkdir -p "$OUT/$slug"
  for keep in "0:first" "$mid:middle" "$last:last" "$texty:text" $(echo "$asked" | tr ',' '\n' | grep . | sed 's/$/:asked/'); do
    f=${keep%%:*}; why=${keep##*:}
    [ -z "$f" ] || [ "$f" = "null" ] && continue
    [ -f "$OUT/$slug/$f.png" ] && continue
    idx=-1
    for i in "${!list[@]}"; do [ "${list[$i]}" = "$f" ] && idx=$i; done
    [ $idx -lt 0 ] && continue
    png=$(printf "%s/r/frame-%05d.png" "$WORK" "$idx")
    [ -f "$png" ] || continue
    cp "$png" "$OUT/$slug/$f.png"
    echo "{\"composition\":\"$rel\",\"id\":$(echo "$probe" | jq -c '.id' 2>/dev/null | grep . || echo null),\"frame\":$f,\"why\":\"$why\",\"frames\":$total,\"cands\":\"$cands\",\"probed\":$probed,\"file\":\"frames/$slug/$f.png\",\"png_sha256\":\"$(shasum -a 256 "$OUT/$slug/$f.png" | cut -d' ' -f1)\",\"sha256\":\"$SHA\",\"binary\":\"$BIN_SHOWN\",\"page\":\"${RAEA_PAGE:-}\"}" >> "$HERE/manifest.jsonl"
    n=$((n + 1))
  done
  echo "ok $rel ($total frames)"
done <<< "$picked"
echo "P26 baseline: $n frames, $(wc -l < "$HERE/failures.txt" | tr -d ' ') compositions failed (failures.txt)"
