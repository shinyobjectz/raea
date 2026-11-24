#!/bin/bash
# The plugin catalog's and the native host's third-party crates' licences (docs/release.feature RL2). The catalog (engine/bevy-plugins, every
# declared plugin on) pulls in crates a plain raea build doesn't; each of those ships in the public tree with its own
# licence files, copied from its source into LICENSES/<crate>-<version>/, and is listed in NOTICE between the markers
# below, with its version and licence expression. A crate that ships no licence file gets LICENSE.txt naming the
# expression its Cargo.toml declares. raea's own crates and vendor/ (each folder keeps its licence) aren't listed.
#
#   tools/licenses.sh           write LICENSES/ and NOTICE's catalog section
#   tools/licenses.sh --check   exit 1 when they differ from what the lock resolves to (after a plugin is added)
#
# Nothing is built: cargo metadata resolves the workspace twice (every catalog feature, then none) from Cargo.lock.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
check=0
[ "${1:-}" = --check ] && check=1
BEGIN="---- the plugin catalog's crates (tools/licenses.sh; don't edit by hand) ----"
END="---- end of the plugin catalog's crates ----"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
feats=$(sed -n '/^\[features\]/,/^\[/p' engine/bevy-plugins/Cargo.toml | grep -oE '^[a-z0-9_]+ =' | cut -d' ' -f1 | grep -v '^default$' | sed 's|^|raea-bevy-plugins/|' | paste -sd, -)
cargo metadata --format-version 1 --locked --offline --features "$feats" > "$tmp/all.json"
cargo metadata --format-version 1 --locked --offline > "$tmp/base.json"

# the package ids reachable from ROOT's crate in a resolve, not through SKIP's
reach='(.resolve.nodes | map({key: .id, value: [.deps[]?.pkg]}) | from_entries) as $n
  | (.packages | map(select(.name == $root)) | .[0].id) as $r
  | (.packages | map(select(.name == $skip)) | .[0].id) as $s
  | {seen: [$r], todo: [$r]}
  | until(.todo == []; ([.todo[] as $id | $n[$id][] | select(. != $s)] | unique) as $next | ($next - .seen) as $new | {seen: (.seen + $new), todo: $new})
  | .seen[]'
# and the native target's host (engine/rune: Rune and its crates), which ships beside raea (docs/targets.feature T1f)
{ jq -r --arg root raea-bevy-plugins --arg skip - "$reach" "$tmp/all.json"; jq -r --arg root raea-rune-host --arg skip - "$reach" "$tmp/base.json"; } | sort -u > "$tmp/catalog"
# raea's own build as Windows and Linux make it: the macOS raea carries the catalog too (docs/nobuild.feature NB4a),
# and its crates keep their licence files here for it
jq -r --arg root raea-cli --arg skip raea-bevy-plugins "$reach" "$tmp/base.json" | sort > "$tmp/raea"
# third-party only (registry or git): raea's crates and vendor/ are path crates
comm -23 "$tmp/catalog" "$tmp/raea" | grep -E '^(registry|git)\+' > "$tmp/ids" || true
jq -c --slurpfile ids <(jq -R . "$tmp/ids") '.packages[] | select(.id as $i | $ids | index($i)) | {name, version, license, license_file, authors: ((.authors // []) | if length == 0 then "its authors" else join(", ") end), dir: (.manifest_path | sub("/Cargo.toml$"; ""))}' "$tmp/all.json" | sort > "$tmp/crates"

# a crate with no licence file of its own: its declared licence's texts, MIT's and Zlib's with its authors as the
# copyright holders (as each template asks), the others' (fixed texts, no holder in them) in LICENSES/texts/
fallback() { # NAME VERSION EXPRESSION AUTHORS
  local id
  printf '%s %s ships no licence file of its own. Its Cargo.toml declares %s, by %s.\n' "$1" "$2" "$3" "$4"
  for id in $(grep -oE '[A-Za-z][A-Za-z0-9.-]*' <<<"$3" | grep -vxE 'OR|AND|WITH' | sort -u); do
    case $id in
      MIT) printf '\n---- MIT ----\n\nCopyright (c) %s\n\n' "$4"; cat <<'MIT' ;;
Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
MIT
      Zlib) printf '\n---- Zlib ----\n\nCopyright (c) %s\n\n' "$4"; cat <<'ZLIB' ;;
This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held
liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter
it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If
   you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not
   required.
2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original
   software.
3. This notice may not be removed or altered from any source distribution.
ZLIB
      Apache-2.0|MPL-2.0|BSL-1.0) printf '\n---- %s: the text is LICENSES/texts/%s.txt ----\n' "$id" "$id"; cp "$ROOT/tools/licenses/$id.txt" "$out/LICENSES/texts/" ;;
      *) printf '\n---- %s: its text ships beside the files it covers (see this crate'"'"'s source) ----\n' "$id" ;;
    esac
  done
}

out=$tmp/out
mkdir -p "$out/LICENSES/texts"
# texts NOTICE's hand-written section cites (not the catalog's): kept as they are
for keep in Apache-2.0.txt; do [ -f "LICENSES/$keep" ] && cp "LICENSES/$keep" "$out/LICENSES/"; done
{
  echo "$BEGIN"
  echo "The plugin catalog (engine/bevy-plugins, built with tools/bevy-plugins.sh) and the native target's host (engine/rune,"
  echo "raea-rune-host) bring in these crates beyond raea's own build. Each is under its own licence, its files in"
  echo "LICENSES/<crate>-<version>/."
  while read -r c; do
    name=$(jq -r .name <<<"$c") version=$(jq -r .version <<<"$c") dir=$(jq -r .dir <<<"$c")
    lic=$(jq -r '.license // ("see " + (.license_file // "its source"))' <<<"$c")
    d=$out/LICENSES/$name-$version
    mkdir -p "$d"
    found=0
    while IFS= read -r f; do
      cp "$f" "$d/" && found=1
    done < <(find "$dir" -maxdepth 1 -type f \( -iname 'LICEN[CS]E*' -o -iname 'COPYING*' -o -iname 'NOTICE*' -o -iname 'UNLICENSE*' -o -iname 'COPYRIGHT*' \) | sort)
    # a crate from a git checkout (Rune's): its repository's licence too, at the checkout's root above the crate's folder
    case $dir in */checkouts/*/*/*)
      while IFS= read -r f; do
        cp "$f" "$d/repository-$(basename "$f")" && found=1
      done < <(find "$(sed -E 's#(.*/checkouts/[^/]+/[^/]+)/.*#\1#' <<<"$dir")" -maxdepth 1 -type f \( -iname 'LICEN[CS]E*' -o -iname 'COPYING*' \) | sort) ;;
    esac
    lf=$(jq -r '.license_file // empty' <<<"$c")
    [ -n "$lf" ] && [ -f "$dir/$lf" ] && cp "$dir/$lf" "$d/" && found=1
    # licences beside what they cover, in a folder of their own (fonts: OFL, the Ubuntu Font Licence)
    while IFS= read -r f; do
      cp "$f" "$d/$(basename "$(dirname "$f")")-$(basename "$f")" && found=1
    done < <(find "$dir" -mindepth 2 -maxdepth 3 -type f \( -iname 'OFL*.txt' -o -iname 'UFL*.txt' -o -iname '*licen[cs]e*.txt' \) -not -path '*/tests/*' | sort)
    [ $found = 1 ] || fallback "$name" "$version" "$lic" "$(jq -r .authors <<<"$c")" > "$d/LICENSE.txt"
    echo "  $name $version: $lic"
  done < "$tmp/crates"
  echo "$END"
} > "$out/section"

# NOTICE: the section replaced (or added after the rest)
notice() {
  if grep -qF -- "$BEGIN" NOTICE; then
    awk -v b="$BEGIN" -v e="$END" -v s="$out/section" '$0 == b { while ((getline l < s) > 0) print l; skip = 1; next } $0 == e { skip = 0; next } !skip' NOTICE
  else
    cat NOTICE; echo; cat "$out/section"
  fi
}
notice > "$out/NOTICE"

if [ $check = 1 ]; then
  status=0
  cmp -s "$out/NOTICE" NOTICE || { echo "licenses: NOTICE's catalog section is stale"; status=1; }
  diff -rq "$out/LICENSES" LICENSES > "$tmp/diff" 2>&1 || { echo "licenses: LICENSES/ differs:"; head -10 "$tmp/diff"; status=1; }
  [ $status = 0 ] && echo "licenses: ok ($(wc -l < "$tmp/crates" | tr -d ' ') catalog crates)"
  exit $status
fi
rm -rf LICENSES
cp -R "$out/LICENSES" LICENSES
cp "$out/NOTICE" NOTICE
echo "licenses: $(wc -l < "$tmp/crates" | tr -d ' ') catalog crates into LICENSES/ and NOTICE ($(du -sh LICENSES | cut -f1))"
