#!/bin/bash
# Names of other businesses' Sprites orgs, and of machines retired from them, appear nowhere in raea but in the list
# that denies them: private/cloud/sprites-denied.txt, which deploy/sprites/org.sh reads too (the 11-04 rule: nothing in
# raea runs on, is named for or points at another business). L1 runs it. It reads the words from the list itself, so
# it names none of them, and where the list isn't (a public clone, public CI) there is nothing to check. (The Modal
# list, modal-denied.txt, isn't read yet: bench case names still carry its word.)
#   tools/denied.sh   check every file of the working tree: public and private paths alike, the files Git ignores
#                     (node_modules, target) and the repositories nested in it (the feed, skills, projects) left out
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
LIVE=${RAEA_LIVE:-$HOME/raea}
list=$LIVE/private/cloud/sprites-denied.txt
[ -f "$list" ] || { echo "denied: no deny list here ($list), nothing to check"; exit 0; }
pat=$(mktemp)
trap 'rm -f "$pat"' EXIT
grep -v -e '^#' -e '^[[:space:]]*$' "$list" | sort -u > "$pat"
[ -s "$pat" ] || exit 0
# the other repositories in the tree are checked by their own
skip=()
while read -r d; do skip+=(-g "!${d#./}/**"); done < <(cd "$ROOT" && find . -maxdepth 4 -name .git -not -path ./.git -not -path ./private/.git | sed 's#/\.git$##')
# --no-ignore-exclude: the private paths are hidden from the public repository by .git/info/exclude, not .gitignore
hits=$(cd "$ROOT" && rg -n -i -F -f "$pat" --no-ignore-exclude --hidden -g '!.git' -g '!private/cloud/sprites-denied.txt' "${skip[@]}" . 2> /dev/null)
if [ -n "$hits" ]; then
  echo "denied: a name from private/cloud/sprites-denied.txt (another business's org, or a machine retired from one):" >&2
  echo "$hits" | cut -c1-200 >&2
  exit 1
fi
echo "denied: none of the $(wc -l < "$pat" | tr -d ' ') denied names outside their list"
