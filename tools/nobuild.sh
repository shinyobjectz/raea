#!/bin/bash
# What makes raea need a Rust build, and what each cause becomes in TypeScript (docs/nobuild.feature NB0, NB7).
#
#   tools/nobuild.sh builds [DAYS]   why raea was built over the last DAYS (14): every antsy run whose command is a cargo or
#                                    lane build, sorted by its stated purpose (--for) into the baseline's classes; with
#                                    NOBUILD_ROWS=1 each class's purposes too, to read by hand
#   tools/nobuild.sh scan            every cause in the code (RAEA_* environment reads, shader constants, cargo features
#                                    of the binaries tools build, declared Bevy plugins), each joined with its decision
#                                    in docs/nobuild/decisions.tsv, written to docs/nobuild/inventory.tsv; prints what
#                                    has no decision yet and the counts by kind and status
#   tools/nobuild.sh consts          each shader constant in the inventory set through <ShaderConstants> in one scene that
#                                    RAEA_BIN (target/release/raea) renders with every pass on: settable (raea set it), refused
#                                    (an array, a vector, not found) or absent (that Engine doesn't load the shader);
#                                    written to docs/nobuild/consts.tsv, NB1a's check for each row
#   tools/nobuild.sh props           each prop in tools/nobuild/props.tsv against the variable it replaced: its scene's
#                                    composition that sets it, and the scene's base with the variable, rendered by
#                                    RAEA_BIN and compared (the frame, and the settings the trace records); written to
#                                    docs/nobuild/props.tsv, exits 1 if one fails
#   tools/nobuild.sh catalog         NB4a's check: RAEA_CATALOG_BIN (a raea with --features catalog, or raea-bevy-sdk built
#                                    so, run as `raea-bevy-sdk motion`) renders and plays tools/nobuild/catalog, a project
#                                    choosing avian and hanabi: the ball falls, the fountain rises, and the trace says
#                                    package.json chose both. Then each props.tsv scene that draws the same pixels twice
#                                    (raster, the path tracer) and tools/nobuild/card.tsx draw the same by it as by RAEA_BIN:
#                                    a plugin compiled in and not chosen changes nothing. Exits 1 if one fails
#   tools/nobuild.sh stats           NB2a's check: every stat variable decided done is read through raea_io_trace::stat;
#                                    tools/nobuild/stats/stats.tsx chooses bevy_passes and trace from TypeScript and reads
#                                    stats.read back; the run prints each pass and keeps stats.json; RAEA_TRACE=1 and
#                                    RAEA_STATS give the same trace lines on the scene without stats.on; pass=depth chosen
#                                    from TypeScript draws RAEA_PASS=depth's frame. Exits 1 if one fails
#
# A decision row (tab-separated): kind:name, then its class, the TypeScript form it becomes, its check, its status
# (open, doing, done, stays). Classes: frame (changes what a frame shows or costs: a prop), view (a debug view: NB2),
# stat (a trace, a timing, a counter: NB2), machine (paths, hosts, keys, threads, decoders: stays an environment
# setting), switch (a kill switch kept for an A/B: removed once decided, or a prop), feature, plugin, shader, glue.
set -uo pipefail
RAEA=${RAEA_ROOT:-$HOME/raea}
DOCS=$RAEA/docs/nobuild
mkdir -p "$DOCS"

builds() {
  local days=${1:-14}
  local hist=${ANTSY_HOME:-$HOME/.antsy}/history.jsonl
  [ -f "$hist" ] || { echo "nobuild: no antsy history at $hist" >&2; exit 1; }
  # the classes in order: the first whose words a purpose has is its class
  jq -rs --argjson days "$days" --arg rows "${NOBUILD_ROWS:-0}" '
    (now - $days * 86400) as $from
    | [.[] | select(.ts >= $from) | select((.cmd // "") | test("cargo (build|run|test|check)\\b|lane build"))]
    | map(. + {class: ((.for // "") | ascii_downcase |
        if test("\\b(a/b|tune|tuning|try|tried|trying|debug|temporar|trace|bisect|time it|timing|timed|measure|threshold|knob|env|logging|eprintln|probe|experiment|variant|without)") then "a value, a toggle, a debug view or a timing"
        elif test("(<[a-z][a-z0-9]*|\\bprops?\\b|component|plugin|parity host|\\bhost\\b|feature|bridge|bevy[_ ]|keyed rows|reflected|rows\\b)") then "a TS-facing feature that needed engine glue"
        elif test("(shader|wgsl|wesl|solari|metalfx|denois|ssao|\\bao\\b|light|shadow|bounce|cache|ray|path tracer|bloom|fog|exposure)") then "shader and lighting work"
        elif test("(test|compile|clippy|lint|\\bl1\\b|\\bci\\b|release|land\\b|landing|rc[0-9]|dupes|fold|help)") then "tests, lint, release and landing"
        else "other engine work" end)})
    | "\(length) builds, \((map(.secs) | add // 0) / 3600 * 10 | floor / 10) h, \(map(.session) | unique | length) sessions, the last \($days) days",
      (group_by(.class) | sort_by(-length)[] |
        "  \(length | tostring | (" " * (5 - length)) + .)  \((map(.secs) | add) / 3600 * 10 | floor / 10) h  \(.[0].class)",
        (if $rows == "1" then (map(.for // "") | unique[] | "        " + .[0:140]) else empty end))' "$hist"
}

# the causes, one a line: kind<TAB>name<TAB>where
causes() {
  cd "$RAEA" || exit 1
  # every RAEA_* variable the code names (its first mention), raea's own and its patches to vendored crates: any quoted
  # literal, as many are read through a helper (a switch(name), a settings table) rather than env::var at the name
  # (git grep: tracked and new files, never what .gitignore leaves out: no target/ or node_modules)
  git grep --untracked -nE '"RAEA_[A-Z0-9_]+"' -- '*.rs' ':!vendor' 'vendor/bevy_solari' 'vendor/blitz-dom' \
      'vendor/blitz-paint' 'vendor/parley_engine' 'vendor/bevy_pbr' 'vendor/bevy_ui' |
    awk -F: '{ w = $1 ":" $2; s = $0; while (match(s, /"RAEA_[A-Z0-9_]+"/)) { print "env\t" substr(s, RSTART + 1, RLENGTH - 2) "\t" w; s = substr(s, RSTART + RLENGTH) } }' |
    sort -t$'\t' -k2,2 -u
  # constants in raea's and the vendored Solari's shaders (bench/ is LightBench's own)
  git grep --untracked -nE '^[[:space:]]*const [A-Z][A-Z0-9_]*[[:space:]]*(:|=)' -- 'engine/*.wgsl' 'engine/*.wesl' 'systems/*.wgsl' 'systems/*.wesl' \
      'vendor/bevy_solari/*.wgsl' 'vendor/bevy_solari/*.wesl' |
    sed -E 's/^([^:]+):([0-9]+):[[:space:]]*const ([A-Z][A-Z0-9_]*).*/shader\t\3\t\1:\2/' | sort -t$'\t' -k2,2 -k3,3 -u
  # optional features of every binary raea ships or its sessions build (each a binary that differs from the shipped one)
  local t
  for t in engine/cli/Cargo.toml engine/page/host/Cargo.toml engine/page/window/Cargo.toml apps/game/Cargo.toml; do
    awk -v f="$t" '/^\[features\]/{p=1; next} /^\[/{p=0} p && /^[a-z0-9_-]+ *=/ && $1 != "default" {print "feature\t" f "#" $1 "\t" f ":" NR}' "$t"
  done
  # places that tell someone to run cargo: the skill (the submodule's own files), and a message or doc in the code
  { grep -rnE 'cargo (build|run|install)' skills/skills 2>/dev/null; git grep -nE '"[^"]*cargo (build|run)[^"]*"' -- '*.rs' ':!bench' ':!tools'; } |
    awk -F: '{ print "advice\t" $1 ":" $2 "\t" $1 ":" $2 }'
  # what the scan can't find, written down as it's found (docs/nobuild/manual.tsv)
  [ -f docs/nobuild/manual.tsv ] && grep -v '^#' docs/nobuild/manual.tsv
  # Bevy plugins a project may declare (engine/bevy-plugins' declared features: each needs tools/bevy-plugins.sh build)
  awk '/>>> declared features/{p=1; next} /<<< declared features/{p=0} p && /^[a-z0-9_-]+ *=/{split($0, a, " "); print "plugin\t" a[1] "\tengine/bevy-plugins/Cargo.toml:" NR}' engine/bevy-plugins/Cargo.toml
}

scan() {
  local dec=$DOCS/decisions.tsv out=$DOCS/inventory.tsv
  [ -f "$dec" ] || printf '# kind:name\tclass\tts_form\tcheck\tstatus\n' > "$dec"
  causes > "$DOCS/.causes.tsv"
  # join each cause with its decision (kind:name); a cause with none is "open", class "?"
  awk -F'\t' -v OFS='\t' '
    FNR == NR { if ($0 !~ /^#/ && NF >= 2) d[$1] = $2 OFS $3 OFS $4 OFS $5; next }
    { k = $1 ":" $2; if (k in d) print $1, $2, $3, d[k]; else print $1, $2, $3, "?", "", "", "open" }
  ' "$dec" "$DOCS/.causes.tsv" > "$out.tmp"
  { printf 'kind\tname\twhere\tclass\tts_form\tcheck\tstatus\n'; sort "$out.tmp"; } > "$out"
  rm -f -- "${out:?}.tmp" "${DOCS:?}/.causes.tsv"
  echo "nobuild: $(($(wc -l < "$out") - 1)) causes in ${out#"$RAEA"/}"
  awk -F'\t' 'NR > 1 { n[$1 " " $4]++ } END { for (k in n) printf "  %4d  %s\n", n[k], k }' "$out" | sort -k2
  awk -F'\t' 'NR > 1 { s[$7]++ } END { for (k in s) printf "  status %s: %d\n", k, s[k] }' "$out" | sort
  local undecided
  undecided=$(awk -F'\t' 'NR > 1 && $4 == "?"' "$out" | wc -l | tr -d ' ')
  [ "$undecided" = 0 ] || echo "nobuild: $undecided causes have no decision (class ?) in docs/nobuild/decisions.tsv"
}

# a shader file's path as Bevy embeds it: its crate's name (dashes as underscores), then its path under src
embedded() {
  local f=$1 d n
  d=$(dirname "$f")
  while [ "$d" != . ] && [ ! -f "$d/Cargo.toml" ]; do d=$(dirname "$d"); done
  n=$(awk '/^\[package\]/{p=1; next} /^\[/{p=0} p && /^name *=/{gsub(/[" ]/, "", $0); sub(/^name=/, ""); print; exit}' "$d/Cargo.toml" 2>/dev/null)
  [ -n "$n" ] && [ "${f#"$d"/src/}" != "$f" ] && printf '%s/%s\n' "${n//-/_}" "${f#"$d"/src/}"
}

consts() {
  local bin=${RAEA_BIN:-$RAEA/target/release/raea} inv=$DOCS/inventory.tsv out=$DOCS/consts.tsv
  [ -x "$bin" ] || { echo "nobuild: no raea at $bin (RAEA_BIN)" >&2; exit 1; }
  [ -f "$inv" ] || scan > /dev/null
  local dir; dir=$(mktemp -d "${TMPDIR:-/tmp}/nobuild-consts.XXXXXX") || exit 1
  cd "$RAEA" || exit 1
  # each constant: its shader as embedded, its name, and its own value where it's a literal (else 1: only its type is
  # checked, and the scene draws nothing that reads it)
  awk -F'\t' 'NR > 1 && $1 == "shader" { print $2 "\t" $3 }' "$inv" | while IFS=$'\t' read -r name where; do
    local file=${where%:*} line=${where##*:} rel expr v
    rel=$(embedded "$file") || { printf '%s\t%s\t-\tnot under a crate src\n' "$name" "$file"; continue; }
    expr=$(sed -n "${line}p" "$file" | sed -E 's/^[^=]*=[[:space:]]*//; s/;.*$//')
    if [[ $expr =~ ^-?[0-9]+(\.[0-9]*)?(e-?[0-9]+)?[uifh]?$ ]]; then v=${expr%[uifh]}
    elif [ "$expr" = true ] || [ "$expr" = false ]; then v=$expr
    # an array or a vector: its own values, or as many 1s when they aren't plain numbers
    elif [[ $expr =~ ^(array|vec)[^\(]*\((.*)\)$ ]]; then
      v=$(printf '%s' "${BASH_REMATCH[2]}" | awk -F, '{ s = ""; for (i = 1; i <= NF; i++) { x = $i; gsub(/[[:space:]uifh]/, "", x); if (x !~ /^-?[0-9.]+(e-?[0-9]+)?$/) x = 1; s = s (i > 1 ? ", " : "") x } print "[" s "]" }')
    else v=1; fi
    printf '%s\t%s\t%s\n' "$name" "$rel" "$v"
  done > "$dir/rows.tsv"
  { printf 'import { Composition, registerRoot } from "raea";\nimport { Camera3d } from "raea/camera";\nimport { ShaderConstants } from "raea/pbr";\n'
    printf 'function Scene() {\n  return (\n    <>\n      <Camera3d />\n'
    awk -F'\t' '$3 != "-" { v[$2] = v[$2] (v[$2] ? ", " : "") $1 ": " $3 } END { for (s in v) printf "      <ShaderConstants shader=\"%s\" values={{ %s }} />\n", s, v[s] }' "$dir/rows.tsv" | sort
    printf '    </>\n  );\n}\nregisterRoot(() => <Composition id="consts" component={Scene} width={64} height={64} fps={30} durationInFrames={1} />);\n'
  } > "$dir/scene.tsx"
  # every pass that embeds shaders switched on: Solari, raea's traced passes (AO, GI's probes, the far sun) and the sun pass
  (cd "$dir" && RAEA_LIGHT=solari RAEA_TRACED_AO=1 RAEA_GI=1 RAEA_SUN_FAR=1 RAEA_PASS=sun "$bin" render scene.tsx --frame 0 -o "$dir/f.png" > /dev/null 2> "$dir/err.log")
  local set; set=$(grep -h "compiled in at load:" "$dir/err.log")
  while IFS=$'\t' read -r name rel v; do
    local got
    if [ "$v" = - ]; then got=$rel; rel=-
    elif grep -qF "no engine shader at $rel " "$dir/err.log"; then got="absent: this Engine doesn't load it"
    elif grep -qF "$rel: no \`const $name\`" "$dir/err.log"; then got="refused: not found"
    elif grep -qF "$rel: $name " "$dir/err.log"; then got="refused: $(grep -F "$rel: $name " "$dir/err.log" | head -1 | sed -E "s|.*$rel: ||; s/; left as.*//" | cut -c1-60)"
    elif printf '%s' "$set" | grep -qF "$rel $name "; then got=settable
    else got="unknown (see $dir/err.log)"; fi
    printf '%s\t%s\t%s\n' "$name" "$rel" "$got"
  done < "$dir/rows.tsv" | sort -u > "$out"
  echo "nobuild: $(wc -l < "$out" | tr -d ' ') shader constants in ${out#"$RAEA"/}, set through <ShaderConstants> by $bin"
  cut -f3 "$out" | sed 's/:.*//' | sort | uniq -c
  grep -q "unknown" "$out" || rm -rf -- "${dir:?}"
}

# a frame's difference from another in 8-bit levels: ImageMagick's METRIC (MAE the mean, PAE the largest), normalised
diff_levels() {
  magick compare -metric "$1" "$2" "$3" null: 2>&1 | sed -nE 's/.*\(([0-9.e+-]+)\).*/\1/p' | awk '{ printf "%.3f", $1 * 255 }'
}

# one render of a props.tsv scene: its composition on its light path with the variables given, into OUT; beside it
# OUT.settings, the run's settings records (fog_settings, solari_settings) without what names the run. The light path:
# raster (as the scene asks), solari (RAEA_LIGHT=solari on every camera), traced (the scene asks for Solari or the
# path tracer itself)
props_render() {
  local scene=$1 comp=$2 light=$3 out=$4; shift 4
  local tdir=$out.trace l=()
  rm -rf -- "${out:?}" "${tdir:?}"
  mkdir -p "$tdir"
  [ "$light" = solari ] && l=(RAEA_LIGHT=solari)
  (cd "$RAEA/tools/nobuild" && env RAEA_TRACE_DIR="$tdir" ${l[@]+"${l[@]}"} "$@" "$bin" render "$scene" --composition "$comp" -o "$out" > /dev/null 2> "$out.err")
  find "$tdir" -name events.jsonl -exec cat {} + 2>/dev/null | jq -c 'select(.kind == "fog_settings" or .kind == "solari_settings") | del(.camera, .view, .env, .t, .ts, .seq, .span)' | sort -u > "$out.settings"
}

props() {
  bin=${RAEA_BIN:-$RAEA/target/release/raea}
  local rows=$RAEA/tools/nobuild/props.tsv out=$DOCS/props.tsv
  [ -x "$bin" ] || { echo "nobuild: no raea at $bin (RAEA_BIN)" >&2; exit 1; }
  local dir; dir=$(mktemp -d "${TMPDIR:-/tmp}/nobuild-props.XXXXXX") || exit 1
  printf '# scene\tcomposition\tagainst\tvariables\tlight\tframe\tprop vs variable (mean, max levels)\tnoise (against twice, mean)\tprop vs against (mean)\tsettings in the trace\tverdict\n' > "$out"
  local fails=0
  while IFS=$'\t' read -r scene comp against vars light frame; do
    case $scene in '#'* | '') continue ;; esac
    local f; f=$(printf 'frame-%05d.png' "$frame")
    local b="$dir/${scene%.tsx}-$against-$light"
    # the composition the variables go on, twice, once a scene and light path: their difference is the run-to-run noise
    if [ ! -d "$b" ]; then
      props_render "$scene" "$against" "$light" "$b"
      props_render "$scene" "$against" "$light" "$b.2"
    fi
    local p="$dir/${scene%.tsx}-$comp-prop" v="$dir/${scene%.tsx}-$comp-var"
    props_render "$scene" "$comp" "$light" "$p"
    # shellcheck disable=SC2086 # the variables are words
    props_render "$scene" "$against" "$light" "$v" $vars
    local mae pae noise moved trace verdict
    mae=$(diff_levels MAE "$p/$f" "$v/$f"); pae=$(diff_levels PAE "$p/$f" "$v/$f")
    noise=$(diff_levels MAE "$b/$f" "$b.2/$f"); moved=$(diff_levels MAE "$p/$f" "$b/$f")
    # (a setting with no record of its own, as steady's or the blur's, is checked by its pixels alone)
    if ! cmp -s "$p.settings" "$v.settings"; then trace=differ; elif [ -s "$p.settings" ]; then trace=same; else trace=none; fi
    # raster draws the same pixels twice: the prop and the variable must too; a traced frame differs run to run, so
    # theirs must be as near as two runs of the same and the prop must move the frame well past that
    verdict=$(awk -v l="$light" -v mae="$mae" -v pae="$pae" -v n="$noise" -v m="$moved" -v t="$trace" -v img="$p/$f" 'BEGIN {
      if (mae == "" || m == "") { print "no frame (" img ")"; exit }
      same = l == "raster" ? pae == 0 : mae <= 2 * n + 0.002
      moves = l == "raster" ? m > 0 : m > 4 * n + 0.01
      print (same && moves && t != "differ") ? "pass" : "FAIL" (same ? "" : ": prop and variable differ") (moves ? "" : ": the prop changes nothing") (t != "differ" ? "" : ": settings differ") }')
    [ "$verdict" = pass ] || fails=$((fails + 1))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s, %s\t%s\t%s\t%s\t%s\n' "$scene" "$comp" "$against" "$vars" "$light" "$frame" "$mae" "$pae" "$noise" "$moved" "$trace" "$verdict" | tee -a "$out"
  done < "$rows"
  echo "nobuild: $(grep -vc '^#' "$out") props against their variables in ${out#"$RAEA"/}, $fails failing, by $bin"
  if [ "$fails" = 0 ]; then rm -rf -- "${dir:?}"; else echo "nobuild: renders kept in $dir"; exit 1; fi
}

# a frame's pixels of one colour, by a rule on r, g, b: how many, and their mean x and y
pixels() {
  magick "$1" -depth 8 txt:- | awk -F'[,:() ]+' -v rule="$2" 'NR > 1 { x = $1; y = $2; r = $3; g = $4; b = $5
    hit = rule == "ball" ? (r > 90 && g < 60 && b > 15 && b < r * 0.6) : (r > 170 && g > 70 && b < 90 && g < r * 0.85)
    if (hit) { n++; sx += x; sy += y } } END { printf "%d %.0f %.0f", n, n ? sx / n : -1, n ? sy / n : -1 }'
}

catalog() {
  bin=${RAEA_BIN:-$RAEA/target/release/raea}
  local cbin=${RAEA_CATALOG_BIN:-$RAEA/target/release/raea-bevy-sdk} here=$RAEA/tools/nobuild/catalog dir fails=0
  [ -x "$bin" ] && [ -x "$cbin" ] || { echo "nobuild: needs raea at $bin (RAEA_BIN) and one with the catalog at $cbin (RAEA_CATALOG_BIN)" >&2; exit 1; }
  dir=$(mktemp -d "${TMPDIR:-/tmp}/nobuild-catalog.XXXXXX") || exit 1
  case $(basename "$cbin") in raea-bevy-sdk*) printf '#!/bin/bash\nexec "%s" motion "$@"\n' "$cbin" > "$dir/craea" ;; *) printf '#!/bin/bash\nexec "%s" "$@"\n' "$cbin" > "$dir/craea" ;; esac
  chmod +x "$dir/craea"
  check() { if [ "$2" = pass ]; then echo "  $1: pass ($3)"; else echo "  $1: FAIL ($3)"; fails=$((fails + 1)); fi; }
  # every frame, as physics steps by the frame drawn (a render --at skips the steps between)
  (cd "$here" && RAEA_TRACE_DIR="$dir/trace" "$dir/craea" render drop.tsx -o "$dir/r" > "$dir/r.log" 2>&1)
  local b0 b40 o40 chosen
  b0=$(pixels "$dir/r/frame-00000.png" ball); b40=$(pixels "$dir/r/frame-00040.png" ball); o40=$(pixels "$dir/r/frame-00040.png" fountain)
  echo "nobuild catalog: tools/nobuild/catalog/drop.tsx by $cbin"
  check "the ball falls (avian)" "$(awk -v a="$b0" -v b="$b40" 'BEGIN { split(a, p, " "); split(b, q, " "); print (p[1] > 100 && q[1] > 100 && q[3] > p[3] + 40) ? "pass" : "x" }')" "frame 0: $b0, frame 40: $b40 (pixels, x, y)"
  check "the fountain rises (hanabi)" "$(awk -v o="$o40" 'BEGIN { split(o, p, " "); print (p[1] > 100 && p[2] > 240) ? "pass" : "x" }')" "frame 40: $o40"
  chosen=$(find "$dir/trace" -name events.jsonl -exec cat {} + 2>/dev/null | jq -r 'select(.kind == "plugins_chosen") | [.chosen[] | "\(.plugin) by \(.because)"] | join(", ")' | sort -u | head -1)
  check "package.json chose them" "$(grep -q 'avian by package.json' <<<"$chosen" && grep -q 'hanabi by package.json' <<<"$chosen" && echo pass)" "${chosen:-no plugins_chosen in the trace}"
  (cd "$here" && "$dir/craea" play drop.tsx --offscreen --no-vsync --size 480x270 --seconds 1.6 --snapshot 1.3 "$dir/play.png" > "$dir/play.log" 2>&1)
  local pb po; pb=$(pixels "$dir/play.png" ball); po=$(pixels "$dir/play.png" fountain)
  check "it plays" "$(awk -v a="$b0" -v b="$pb" -v o="$po" 'BEGIN { split(a, p, " "); split(b, q, " "); split(o, f, " "); print (q[1] > 100 && q[3] > p[3] + 40 && f[1] > 100) ? "pass" : "x" }')" "at 1.3 s: ball $pb, fountain $po"
  # the same pixels with the catalog compiled in and nothing chosen
  local scene comp light frame f
  while IFS=$'\t' read -r scene comp light frame; do
    f=$(printf 'frame-%05d.png' "$frame")
    props_render "$scene" "$comp" "$light" "$dir/plain-${scene%.tsx}-$comp"
    bin=$dir/craea props_render "$scene" "$comp" "$light" "$dir/cat-${scene%.tsx}-$comp"
    local pae; pae=$(diff_levels PAE "$dir/plain-${scene%.tsx}-$comp/$f" "$dir/cat-${scene%.tsx}-$comp/$f")
    check "$scene $comp, frame $frame: the same pixels" "$([ "$pae" = 0.000 ] && echo pass)" "max ${pae:-no frame} levels"
  done < <({ awk -F'\t' '$1 !~ /^#/ && NF >= 6 && ($5 == "raster" || ($5 == "traced" && $3 == "trace")) { print $1 "\t" $3 "\t" $5 "\t" $6 }' "$RAEA/tools/nobuild/props.tsv"; printf 'card.tsx\tcard\traster\t20\n'; } | sort -u)
  echo "nobuild catalog: $fails failing"
  if [ "$fails" = 0 ]; then rm -rf -- "${dir:?}"; else echo "nobuild: renders and logs kept in $dir"; exit 1; fi
}

stats() {
  bin=${RAEA_BIN:-$RAEA/target/release/raea}
  local here=$RAEA/tools/nobuild/stats dir fails=0 v raw="" n=0
  [ -x "$bin" ] || { echo "nobuild: needs raea at $bin (RAEA_BIN)" >&2; exit 1; }
  dir=$(mktemp -d "${TMPDIR:-/tmp}/nobuild-stats.XXXXXX") || exit 1
  check() { if [ "$2" = pass ]; then echo "  $1: pass ($3)"; else echo "  $1: FAIL ($3)"; fails=$((fails + 1)); fi; }
  echo "nobuild stats: tools/nobuild/stats/stats.tsx by $bin"
  # each stat or view variable decided done is read through stat() or stat_value(), so TypeScript and RAEA_STATS reach it
  for v in $(awk -F'\t' '($2 == "stat" || $2 == "view") && $5 == "done" && $1 ~ /^env:/ { sub(/^env:/, "", $1); print $1 }' "$RAEA/docs/nobuild/decisions.tsv"); do
    n=$((n + 1)); raw+=$(cd "$RAEA" && git grep -n -E "env::var(_os)?\(\"$v\"\)" -- '*.rs' ':!vendor' ':!io/trace' | head -2)
  done
  check "each done stat or view variable read through stat()" "$([ -z "$raw" ] && echo pass)" "${raw:-$n variables}"
  # chosen from TypeScript, no variable set: stats.on at the scene's top level, stats.read on its last frame
  (cd "$here" && env -u RAEA_TRACE -u RAEA_BEVY_PASSES -u RAEA_STATS RAEA_TRACE_DIR="$dir/trace" "$bin" render stats.tsx -o "$dir/ts" > "$dir/ts.log" 2>&1)
  local read rows kept
  # the trace channel's lines (RAEA_TRACE's: which path drew the frame), one of each
  traced() { grep -E '^trace ' "$1" | sort -u; }
  check "stats.on turned trace on" "$([ -n "$(traced "$dir/ts.log")" ] && echo pass)" "$(traced "$dir/ts.log" | head -1)"
  # the last frame's, as the engine draws it (a render evaluates the edit's frames ahead of the engine too)
  read=$(grep '^stats.read:' "$dir/ts.log" | tail -1)
  check "stats.read from TypeScript" "$(awk '{ for (i = 1; i < NF; i++) if ($(i + 1) == "bevy" && $i + 0 > 0) ok = 1 } END { print ok ? "pass" : "x" }' <<<"$read")" "${read:-no stats.read line}"
  rows=$(grep -c '^  bevy/' "$dir/ts.log")
  check "the run's end printed each pass" "$([ "$rows" -gt 0 ] && grep -q '^raea stats (bevy_passes, trace):' "$dir/ts.log" && echo pass)" "$rows of Bevy's passes"
  kept=$(find "$dir/trace" -name stats.json -exec jq -r '[.passes | keys[] | select(startswith("bevy/"))] | length' {} + 2>/dev/null | head -1)
  check "stats.json kept in the run's trace" "$([ "${kept:-0}" -gt 0 ] && echo pass)" "${kept:-no stats.json}: Bevy's passes"
  # the overrides, on the scene without stats.on: the channel's own variable, and RAEA_STATS
  sed '/^native("stats.on"/d' "$here/stats.tsx" > "$dir/plain.tsx"
  (cd "$dir" && env -u RAEA_STATS RAEA_TRACE=1 "$bin" render plain.tsx -o "$dir/env" > "$dir/env.log" 2>&1)
  (cd "$dir" && env -u RAEA_TRACE RAEA_STATS=trace,bevy_passes "$bin" render plain.tsx -o "$dir/list" > "$dir/list.log" 2>&1)
  (cd "$dir" && env -u RAEA_TRACE -u RAEA_BEVY_PASSES -u RAEA_STATS "$bin" render plain.tsx -o "$dir/none" > "$dir/none.log" 2>&1)
  check "nothing chosen: no trace lines, no table" "$([ -z "$(traced "$dir/none.log")" ] && ! grep -q '^raea stats' "$dir/none.log" && grep -q '^APPLIED' "$dir/none.log" && echo pass)" "$(traced "$dir/none.log" | wc -l | tr -d ' ') trace lines"
  # a debug view chosen with its value: pass=depth from TypeScript draws RAEA_PASS=depth's frame, not the beauty
  sed 's/channels: \["bevy_passes", "trace"\]/channels: ["pass=depth"]/' "$here/stats.tsx" > "$dir/view.tsx"
  (cd "$dir" && env -u RAEA_PASS -u RAEA_STATS "$bin" render view.tsx -o "$dir/view" > "$dir/view.log" 2>&1)
  (cd "$dir" && env -u RAEA_STATS RAEA_PASS=depth "$bin" render plain.tsx -o "$dir/penv" > "$dir/penv.log" 2>&1)
  local pae beauty
  pae=$(diff_levels PAE "$dir/view/frame-00006.png" "$dir/penv/frame-00006.png"); beauty=$(diff_levels PAE "$dir/view/frame-00006.png" "$dir/none/frame-00006.png")
  check "pass=depth from TypeScript draws RAEA_PASS=depth's frame" "$([ "$pae" = 0.000 ] && [ -n "$beauty" ] && [ "$beauty" != 0.000 ] && echo pass)" "max ${pae:-no frame} levels apart; ${beauty:-no frame} from the beauty"
  check "RAEA_TRACE=1 the same trace lines" "$([ "$(traced "$dir/env.log")" = "$(traced "$dir/ts.log")" ] && ! grep -q '^raea stats' "$dir/env.log" && echo pass)" "$(traced "$dir/env.log" | wc -l | tr -d ' ') lines, no table"
  check "RAEA_STATS=trace,bevy_passes the same, with the table" "$([ "$(traced "$dir/list.log")" = "$(traced "$dir/ts.log")" ] && grep -q '^  bevy/' "$dir/list.log" && echo pass)" "$(traced "$dir/list.log" | wc -l | tr -d ' ') lines, $(grep -c '^  bevy/' "$dir/list.log") passes"
  echo "nobuild stats: $fails failing"
  if [ "$fails" = 0 ]; then rm -rf -- "${dir:?}"; else echo "nobuild: renders and logs kept in $dir"; exit 1; fi
}

case ${1:-} in
  builds) shift; builds "$@" ;;
  scan) scan ;;
  consts) consts ;;
  props) props ;;
  catalog) catalog ;;
  stats) stats ;;
  *) sed -n '2,34p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
