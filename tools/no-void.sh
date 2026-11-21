#!/bin/bash
# "void" is retired: raea's application layer is raea feed (decided 10-25). This ratchet keeps the old name from
# coming back. It counts the lines of each tracked file (raea's and apps/app's) that use "void" as a product name: the
# names raea-void, void-bevy, void_bevy, void-app, void-conform and RAEA_VOID_*, and the bare word wherever it isn't
# TypeScript's `void` type or operator. It fails when a file has more such lines than tools/no-void.allow gives it, or
# a file it doesn't list has any. The allowance only shrinks: rename, then take the new counts.
#   tools/no-void.sh            check; each file over its allowance, with its lines
#   tools/no-void.sh --update   tools/no-void.allow rewritten to today's counts (refused if any count rose)
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 2
ALLOW=tools/no-void.allow
NAMES='raea-void|void[-_](bevy|app|conform|shell)|RAEA_VOID_'
# TypeScript's void (and C's, in a few FFI comments): a type after `:`, `=>`, `<` or `|`, or the operator before an
# expression (`void css;` too, in TS a script writes); ts-native's checklist names the operator's own tests; those
# lines aren't the product
KEYWORD='(:|=>|<|\||\(|,) *void\b|\bvoid *(0|\(|>|\||;|,|\)|\]|\{|=)|\bvoid [a-z_$][a-zA-Z0-9_$.]*\(|\bvoid [A-Za-z_$][A-Za-z0-9_$]*;|operators\.void|expressions/void|c_void|void\*|\bvoid main\b|no-void'
# the fallback readers: they read RAEA_VOID_* and move ~/.raea-void for one release, so they must
# name both; dropped from here when the fallback is
SHIMS=(':!systems/ml/src/feed.rs')
# text that isn't ours or is a record, kept as written:
#   the fonts' OFL licence texts (the licence's own word, "void")
#   a recorded SEO rankings file (the keywords searched that day)
#   P26 baseline rows already taken (their notes name the composition as it was then)
RECORDS=(':!*-OFL.txt' ':!apps/site/seo/rankings/*' ':!bench/p26-baseline/manifest.jsonl')
# raea-skills (skills/) is its own repo: an export has its files, the live tree's git grep never reads them
SKIP=("${SHIMS[@]}" "${RECORDS[@]}" ':!skills/*' ':!*.lock' ':!*pnpm-lock.yaml' ':!*package-lock.json' ':!*/vendor/*' ':!vendor/*' ':!**/node_modules/**' ':!tools/no-void.allow' ':!tools/no-void.sh' ':!docs/handoff/*' ':!docs/overhaul/audit/*' ':!*LICENSE*' ':!*LICENCE*' ':!*.png' ':!*.jpg' ':!*.mp4' ':!*.woff2' ':!*.glb' ':!*.wasm')

# file:line of each use, in one repo (prefixed with its path in raea)
uses() {
  local dir=$1 pre=$2
  {
    git -C "$dir" grep -n -I -E "$NAMES" -- . "${SKIP[@]}"
    git -C "$dir" grep -n -I -i -w void -- . "${SKIP[@]}" ':!*.ts' ':!*.tsx' ':!*.js' ':!*.mjs' ':!*.cjs' ':!*.d.ts' \
      | grep -v -i -E "^[^:]+:[0-9]+:.*($KEYWORD)"
  } | awk -F: -v p="$pre" '{print p $1 ":" $2}' | sort -u
}
all=$(uses . ""; [ -e apps/app/.git ] && uses apps/app "apps/app/")
counts=$(printf '%s\n' "$all" | grep -v '^$' | awk -F: '{n[$1]++} END {for (f in n) print n[f], f}' | sort -k2)

if [ "${1:-}" = "--update" ]; then
  rose=0
  while read -r n f; do
    [ -n "$n" ] || continue
    a=$(awk -v f="$f" '$2 == f {print $1}' "$ALLOW" 2>/dev/null)
    if [ -f "$ALLOW" ] && [ "${a:-0}" -lt "$n" ]; then echo "no-void: $f rose to $n (allowed ${a:-0}); rename it instead"; rose=1; fi
  done <<< "$counts"
  [ $rose = 0 ] || exit 1
  printf '%s\n' "$counts" | grep -v '^$' > "$ALLOW"
  echo "no-void: $ALLOW now $(wc -l < "$ALLOW" | tr -d ' ') files, $(awk '{s+=$1} END {print s+0}' "$ALLOW") lines"
  exit 0
fi

bad=0
while read -r n f; do
  [ -n "$f" ] || continue
  a=$(awk -v f="$f" '$2 == f {print $1}' "$ALLOW")
  if [ "${a:-0}" -lt "$n" ]; then
    echo "no-void: $f has $n product uses of \"void\" (allowed ${a:-0}); the app is raea feed:"
    printf '%s\n' "$all" | awk -F: -v f="$f" '$1 == f {print $2}' | while read -r l; do
      echo "  $l: $(sed -n "${l}p" "$f" | cut -c1-140)"
    done
    bad=1
  fi
done <<< "$counts"
total=$(printf '%s\n' "$counts" | awk '{s+=$1} END {print s+0}')
[ $bad = 0 ] && echo "no-void: ok ($total uses left in $(printf '%s\n' "$counts" | grep -c .) files, none new)"
exit $bad
