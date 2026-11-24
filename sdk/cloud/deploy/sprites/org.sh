# shellcheck shell=bash
# The Sprites org raea's machines go in, sourced by deploy/sprites/box.sh, tools/linux-box.sh and tools/linux-check.sh
# (it sets ORG). The org is the one raea owns, written once in ~/.config/raea-cloud/sprites.env (RAEA_SPRITES_ORG=...),
# never taken from the environment, and never one listed in private/cloud/sprites-denied.txt (orgs that belong to other
# businesses). (No apostrophe may go in a ${VAR:?message}: it opens a quote that swallows the lines after it, as it
# once did this guard's, and a box went into the wrong org.)
SPRITES_CONF=${RAEA_SPRITES_ENV:-$HOME/.config/raea-cloud/sprites.env}
ORG=$( [ -f "$SPRITES_CONF" ] && sed -n 's/^RAEA_SPRITES_ORG=//p' "$SPRITES_CONF" | tail -1 || true)
[ -n "$ORG" ] || { echo "sprites: write RAEA_SPRITES_ORG=<the Sprites org raea owns> in $SPRITES_CONF (sprite org list)"; exit 2; }
SPRITES_DENIED=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel 2>/dev/null)/private/cloud/sprites-denied.txt
if [ -f "$SPRITES_DENIED" ] && grep -qxF "$ORG" "$SPRITES_DENIED"; then
  echo "sprites: $ORG belongs to another business ($SPRITES_DENIED); raea machines go in an org raea owns"
  exit 2
fi
