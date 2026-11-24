#!/bin/bash
# L2, the orchestrator's post-landing suites (docs/agents.md "Check levels"), run one way every time:
#
#   tools/l2.sh [SHA]        # default: origin/main when it starts
#
# - It tests one commit, fixed at the start. Landings go on while it runs; they never restart it. The next run
#   takes whatever landed since.
# - It runs in its own checkout ($RAEA_L2_DIR, default ~/raea-trial/a), never in ~/raea's shared tree, so no
#   lane's uncommitted edit reaches it. That checkout keeps its target/: a release build without incremental (the
#   reference build), so a new commit rebuilds only the crates that changed.
# - It resumes: results go to $RAEA_L2_OUT/<sha> (default ~/Library/Caches/raea/l2), and a composition already
#   compared for that commit is skipped. A run that was stopped is started again with the same SHA.
# - P26 runs one composition at a time and prints a line for each, so an antsy --stall sees progress.
# - The checkout gets what git doesn't track (media, fonts, solves, node_modules) as links to ~/raea's copies,
#   made once. A link is never made through a linked folder, so nothing is written into ~/raea.
# - Before building, it sweeps ~/raea's stale debug caches if disk is short, and refuses under 8 GB.
#
# Output: one line per step ("L2 <step>: ..."), then "L2 <sha>: P26 a/b, ui-parity c/d, conformance e/f, ..., bench
# checks g/h" (bench/l2-checks.txt) and $OUT/summary.json. Run it under antsy with a stall of 20 minutes or more (a heavy composition prints after
# rendering all its frames).
set -uo pipefail
ROOT=$HOME/raea
T=${RAEA_L2_DIR:-$HOME/raea-trial/a}
git -C "$T" rev-parse --git-dir > /dev/null 2>&1 || { echo "L2: no checkout at $T (git clone ~/raea's origin there)"; exit 2; }
git -C "$T" fetch -q origin || exit 2
SHA=$(git -C "$T" rev-parse --short=8 "${1:-origin/main}") || exit 2
OUT=${RAEA_L2_OUT:-$HOME/Library/Caches/raea/l2}/$SHA
mkdir -p "$OUT/bin" "$OUT/p26"
free() { df -g / | awk 'NR==2{print $4}'; }
say() { echo "L2 $*"; }

# the commit, and the app: in its tree since 11-16 (a clone of the old raea-feed app left in the checkout goes first);
# before, the raea-feed commit at that commit's pointer, in a clone of its own; a commit of the public repo records
# neither (the app is raea-feed's, outside the export), so raea-feed's main, as L1 reads it (tools/agents/check),
# fetched into the checkout's own clone, never into ~/raea's
intree=; [ "$(git -C "$T" cat-file -t "$SHA:apps/app" 2>/dev/null)" = tree ] && intree=1
[ -n "$intree" ] && [ -d "$T/apps/app/.git" ] && rm -rf "${T:?}/apps/app"
git -C "$T" checkout -q -f "$SHA" || exit 2
[ -z "$(git -C "$T" status --porcelain --untracked-files=no -- . ':!apps/app')" ] || { say "checkout has tracked changes"; exit 2; }
app=$(git -C "$T" ls-tree HEAD apps/app | awk '{print $3}')
if [ -z "$intree" ]; then
  [ -d "$T/apps/app/.git" ] || { rmdir "$T/apps/app" 2>/dev/null; git clone -q "$ROOT/apps/app" "$T/apps/app" || exit 2; }
  if [ -n "$app" ]; then
    git -C "$T/apps/app" fetch -q "$ROOT/apps/app" || exit 2
  else
    git -C "$T/apps/app" fetch -q "$(git -C "$ROOT/apps/app" remote get-url origin)" main || exit 2
    app=$(git -C "$T/apps/app" rev-parse FETCH_HEAD) || exit 2
  fi
  git -C "$T/apps/app" checkout -q -f "$app" || exit 2
fi
say "commit: $(git -C "$T" log --oneline -1 | cut -c1-90) | app ${app:0:8}"

# what git doesn't track, linked from ~/raea (once; never through a link, never trimming a name)
links=0; TREAL=$(cd "$T" && pwd -P)
while IFS= read -r -d '' p; do
  p=${p%/}
  case $p in target|target/*|*/target|*/target/*|*.DS_Store) continue ;; esac
  { [ -e "$T/$p" ] || [ -L "$T/$p" ]; } && continue
  # the nearest folder of it that exists must resolve inside the checkout (else mkdir or ln writes into ~/raea)
  up=$(dirname "$p"); while [ "$up" != . ] && [ ! -d "$T/$up" ]; do up=$(dirname "$up"); done
  case $(cd "$T/$up" && pwd -P)/ in "$TREAL"/*) ;; *) continue ;; esac
  mkdir -p "$T/$(dirname "$p")" && ln -s "$ROOT/$p" "$T/$p" && links=$((links + 1))
done < <(git -C "$ROOT" status --porcelain=v1 -z --ignored -- content sdk bench engine 2>/dev/null | tr '\0' '\n' | sed -n 's/^!! //p' | tr '\n' '\0')
# the app's packages (apps/app is a submodule: its own ignored files aren't in ~/raea's status)
[ -e "$T/apps/app/node_modules" ] || ln -s "$ROOT/apps/app/node_modules" "$T/apps/app/node_modules"
# the app's host reads the private site's code (host/library/local.ts imports apps/site/api's fuse.ts); a public commit
# has no apps/site, so conformance's app build would fail: linked from ~/raea's, as a release builds the app there
[ -e "$T/apps/site" ] || [ ! -d "$ROOT/apps/site" ] || ln -s "$ROOT/apps/site" "$T/apps/site"
# a tracked relative link that climbs out of ~/raea (media/assets/residential -> ../../../../../Assets) resolves
# from the checkout's parent: give that parent the same folders
for d in Assets; do [ -e "$(dirname "$T")/$d" ] || [ ! -e "$HOME/$d" ] || ln -s "$HOME/$d" "$(dirname "$T")/$d"; done
say "links: $links new"

# disk
if [ "$(free)" -lt 15 ]; then bash "$ROOT/tools/target-sweep.sh" --debug --delete --age 60 --target "$ROOT/target" | tail -1; fi
[ "$(free)" -ge 8 ] || { say "refused: $(free) GB free (needs 8)"; exit 1; }

# builds (lane stops a hold at 15 minutes; a stopped cargo build resumes where it left off)
build() {
  local n=$1; shift
  for _ in 1 2 3 4 5 6 7 8; do
    # every 25th crate compiled is a line on stdout: a full release build prints nothing else for half an hour, and
    # an antsy --stall would stop a compiler that's working (10-27)
    (cd "$T" && lane build -- env CARGO_INCREMENTAL=0 cargo build --release -j 6 "$@") 2>&1 </dev/null | tee "$OUT/$n.log" |
      awk -v n="$n" '/^ *Compiling / && ++c % 25 == 0 { print "L2 " n ": " c " crates compiled"; fflush() }'
    grep -q '^ *Finished' "$OUT/$n.log" && { say "$n: $(grep '^ *Finished' "$OUT/$n.log" | sed 's/^ *//' | cut -c1-90)"; return 0; }
    grep -qE '^error(\[|:)' "$OUT/$n.log" && { say "$n: FAILED $(grep -m1 -E '^error' "$OUT/$n.log" | cut -c1-120)"; return 1; }
    # lane refuses a build under its disk floor at once: that is no hold, and a retry is refused the same way (11-22:
    # eight retries in a second, each reported as "lane hold ended")
    grep -q '^lane build: refused' "$OUT/$n.log" && { say "$n: REFUSED $(grep -m1 '^lane build: refused' "$OUT/$n.log" | cut -c13-170)"; return 1; }
    say "$n: lane hold ended, resuming"
  done
  return 1
}
# a resumed run whose binaries were made for this commit already uses them: the build is about 30 GB and half an hour
# again, and 11-22 the disk was emptied between the two
if [ -x "$OUT/bin/raea" ] && [ -x "$OUT/bin/raea-page-host" ] && [ -x "$OUT/bin/conformance" ] && [ -x "$OUT/bin/ui-parity" ]; then
  say "builds: this commit's binaries are made already ($OUT/bin)"
else
  build build-raea -p raea-cli || exit 1
  cp "$T/target/release/raea" "$T/target/release/libraea_bundle.dylib" "$OUT/bin/" || exit 1
  build build-host -p raea-page-host || exit 1
  build build-conformance --manifest-path sdk/ui/conformance/Cargo.toml --target-dir target || exit 1
  build build-parity --manifest-path sdk/ui/parity/Cargo.toml --target-dir target || exit 1
  for b in raea-page-host conformance ui-parity; do cp "$T/target/release/$b" "$OUT/bin/" || exit 1; done
fi
# P26, one composition at a time, skipping those already compared for this commit
cd "$T" || exit 2
# the plugin build's rows go last, after their binary is built, so a long plugin build doesn't hold up the rest
jq -r 'select(.binary | test("bevy-plugins|bin-avian") | not) | .composition' bench/p26-baseline/manifest.jsonl | awk '!s[$0]++' > "$OUT/p26/all"
jq -r 'select(.binary | test("bevy-plugins|bin-avian")) | .composition' bench/p26-baseline/manifest.jsonl | awk '!s[$0]++' > "$OUT/p26/plugin"
p26_rows() { # LIST
while IFS= read -r c; do
  n=$(echo "$c" | tr '/#' '__')
  [ -s "$OUT/p26/$n/results.jsonl" ] && continue
  rm -rf "$OUT/p26/$n"
  RAEA_HOME="$T" ONLY="$c" P26_PLUGIN_BIN="$OUT/bin/plugin.sh" P26_OUT="$OUT/p26/$n" \
    bash bench/p26-baseline/compare.sh "$OUT/bin/raea" </dev/null > "$OUT/p26/$n.log" 2>&1
  grep -hE '^(ok|MISS)' "$OUT/p26/$n.log" | sed 's/^/L2 p26 /' | cut -c1-200
  rm -rf "$OUT/p26/$n/frames"
done < "$1"
}
p26_rows "$OUT/p26/all"

# ui-parity full (calib's off-* cases fail by design) and conformance full, on the page host
RAEA_TS_NATIVE_RESULTS="$OUT/ui-parity/latest.json" "$OUT/bin/ui-parity" run --tier full --jobs 2 --bevy "$OUT/bin/raea-page-host" --out "$OUT/ui-parity" </dev/null > "$OUT/ui-parity.log" 2>&1
parity=$(jq -rs '"\(map(select(.area != "CALIB" and .pass)) | length)/\(map(select(.area != "CALIB")) | length)"' "$OUT/ui-parity/results.jsonl" 2>/dev/null || echo "no results")
say "ui-parity: $parity (calibration cases left out)"
RAEA_FEED_AGENT=echo "$OUT/bin/conformance" --bin "$OUT/bin/raea-page-host" --substrate blitz --tier full --out "$OUT/conformance" </dev/null > "$OUT/conformance.log" 2>&1
conf=$(jq -r '"\(.pass)/\(.cases)"' "$OUT/conformance/summary.json" 2>/dev/null || echo "no results")
say "conformance: $conf"

# the bench checks in bench/l2-checks.txt, each run with this commit's raea: they test what is right (a frame drawn alone
# or out of order agrees with the render), where P26 only compares with its baseline, which can carry the same fault
ok=0; total=0
while read -r c; do
  case $c in '' | '#'*) continue ;; esac
  total=$((total + 1)); log="$OUT/bench-$(echo "$c" | tr / -).log"
  if RAEA_BIN="$OUT/bin/raea" bash "$c" </dev/null > "$log" 2>&1; then ok=$((ok + 1)); else say "bench miss: $c ($(tail -1 "$log" | cut -c1-140))"; fi
done < bench/l2-checks.txt
benches="$ok/$total"
say "bench checks: $benches"

# the plugin build and its P26 rows go last: a long or refused plugin build holds up none of the suites above
bindings="not built"
cd "$ROOT" || exit 2
# the plugin rows' binary, as tools/bevy-plugins.sh builds it for bench/bevy-plugins (motion.sh)
# it compiles every crate again with the plugins' features: 11-22 it filled a disk that had 8 GB free at its start
if [ "$(free)" -lt 45 ]; then
  say "plugin build: not made, $(free) GB free (it needs about 40), so its $(wc -l < "$OUT/p26/plugin" | tr -d ' ') P26 compositions weren't compared"
elif feats=$(cd "$T" && bash tools/bevy-plugins.sh features sdk/raea/bench/bevy-plugins) && build build-plugins -p raea-cli --bin raea-bevy-sdk --features "$feats" && cp "$T/target/release/raea-bevy-sdk" "$OUT/bin/"; then
  printf '#!/bin/sh\nexec %s motion "$@"\n' "$OUT/bin/raea-bevy-sdk" > "$OUT/bin/plugin.sh" && chmod +x "$OUT/bin/plugin.sh"
  cd "$T" || exit 2
  # the generated plugin bindings (sdk/plugins/src) against this commit's registry, read from the plugin build just
  # made (about 50 s): a landing that registers a reflected type (a material's MeshMaterial3d<..>, say) changes them,
  # and only this build can read the registry, which L1 doesn't make; four such landings went unnoticed 10-31 to 11-08
  # (d708518be)
  if bash tools/bevy-plugins.sh check sdk/raea/bench/bevy-plugins > "$OUT/bindings.log" 2>&1; then bindings=ok; else bindings=drift; fi
  say "plugin bindings: $bindings$([ $bindings = ok ] || echo " (tools/bevy-plugins.sh build sdk/raea/bench/bevy-plugins, then land sdk/plugins/src; $OUT/bindings.log)")"
  p26_rows "$OUT/p26/plugin"
else
  cd "$T" || exit 2
  say "plugin build: not made, so its $(wc -l < "$OUT/p26/plugin" | tr -d ' ') P26 compositions weren't compared (see above)"
fi
# a composition ONLY matched by substring also ran its longer-named neighbours: keep one row per frame
cat "$OUT"/p26/*/results.jsonl | jq -s 'unique_by(.composition, .frame)' > "$OUT/p26.json"
p26=$(jq -r '"\(map(select(.pass)) | length)/\(length)"' "$OUT/p26.json")
jq -r 'map(select(.pass | not)) | .[] | "L2 p26 miss: \(.composition) @\(.frame) \(.ssim)"' "$OUT/p26.json"

jq -n --arg sha "$SHA" --arg p26 "$p26" --arg parity "$parity" --arg conf "$conf" --arg bindings "$bindings" --arg benches "$benches" '{sha: $sha, p26: $p26, ui_parity: $parity, conformance: $conf, plugin_bindings: $bindings, bench_checks: $benches}' > "$OUT/summary.json"
echo "L2 $SHA: P26 $p26, ui-parity $parity, conformance $conf, plugin bindings $bindings, bench checks $benches"
