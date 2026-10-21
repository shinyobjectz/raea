#!/bin/sh
# Package the game library (sdk/motion/docs/game-library.md).
#
#   sdk/motion/library/games/pack.sh lib OUT_DIR
#       the part that ships with the app: catalogue.jsonl, index.bin, index.json, thumbs/, tsx/ (about 2 MB) as
#       OUT_DIR/raea-games-lib-<catalogue fnv>.tar.gz. Installed at ~/.raea/library/games, where raea-meaning finds it
#       when it isn't run from the repo (or point $RAEA_GAMES_LIB at it).
#   sdk/motion/library/games/pack.sh assets OUT_DIR [--starter] [--max-mb N]
#       the optional asset pack, a release artifact: every `ship: download` record (or, with --starter, the rigged
#       characters and animation sets under 8 MB that are checked to render) fetched through `raea-meaning
#       games-fetch` (licence-checked, from the recorded URLs, sha256 checked where recorded) and packed with
#       MANIFEST.json (id, licence, attribution, source, files) and every record's ATTRIBUTION.txt, as
#       OUT_DIR/raea-games-assets[-starter]-<fnv>.tar. Unpack it into ~/.raea/assets/games/ and games-fetch finds
#       the files already there. Never commit it or put it in the app bundle.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
MEANING=${RAEA_MEANING:-$ROOT/ai/meaning/target/release/raea-meaning}
what=${1:?lib|assets}
out=${2:?OUT_DIR}
mkdir -p "$out"
fnv=$(python3 -I -c "import json,sys; print(json.load(open(sys.argv[1]))['catalogue_fnv'])" "$HERE/index.json")

case "$what" in
lib)
  tar -czf "$out/raea-games-lib-$fnv.tar.gz" -C "$HERE" catalogue.jsonl index.bin index.json thumbs tsx
  ls -l "$out/raea-games-lib-$fnv.tar.gz"
  ;;
assets)
  starter=0; max=0
  shift 2
  while [ $# -gt 0 ]; do
    case "$1" in --starter) starter=1 ;; --max-mb) max=$2; shift ;; esac
    shift
  done
  ids=$(python3 -I - "$HERE/catalogue.jsonl" "$starter" "$max" <<'PY'
import json, sys
starter, cap = sys.argv[2] == "1", float(sys.argv[3]) * 1e6
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r.get("ship") != "download":
        continue
    b = r.get("download_bytes") or 0
    if starter and not (r["kind"] in ("rig", "animation") and r.get("main") and b < 8e6 and "fails" not in r.get("raea", "")):
        continue
    if cap and b > cap:
        continue
    print(r["id"])
PY
)
  stage=$(mktemp -d "${TMPDIR:-/tmp}/raea-games-pack.XXXXXX")
  for id in $ids; do
    RAEA_GAMES_ASSETS="$stage" "$MEANING" games-fetch "$id" > /dev/null || { echo "pack: $id failed to fetch" >&2; rm -rf "$stage"; exit 1; }
    # zips stay unpacked only: the archive is the pack's own compression
    find "$stage/$id" -maxdepth 1 -name '*.zip' -delete
  done
  python3 -I - "$HERE/catalogue.jsonl" "$stage" $ids <<'PY'
import json, sys
recs = {json.loads(l)["id"]: json.loads(l) for l in open(sys.argv[1])}
keep = ("id", "kind", "title", "licence", "attribution", "source", "main", "files")
manifest = [{k: recs[i][k] for k in keep if k in recs[i]} for i in sys.argv[3:]]
open(f"{sys.argv[2]}/MANIFEST.json", "w").write(json.dumps(manifest, indent=1, ensure_ascii=False) + "\n")
PY
  name=raea-games-assets$([ $starter = 1 ] && echo -starter || true)-$fnv.tar
  tar -cf "$out/$name" -C "$stage" .
  rm -rf "$stage"
  ls -l "$out/$name"
  ;;
*) echo "pack.sh lib|assets OUT_DIR" >&2; exit 2 ;;
esac
