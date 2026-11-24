#!/bin/bash
# raea cloud's agent box on a Fly Sprite (docs/cloud-render.feature CR11, CR12): raea's Linux release, ACP agents and
# sdk/cloud's server on the Sprite's own HTTPS URL, its renders sent to the farm's GPUs (deploy/modal/farm.py). A Sprite
# bills the CPU and memory it actually uses, sleeps when nothing talks to it and keeps its disk, so an agent that mostly
# waits on its model costs little; the GPUs it needs come from the farm and stop when the render does. From sdk/cloud:
#
#   deploy/sprites/box.sh up NAME      the Sprite, made or completed by deploy/sprites/setup.sh (the script a bot's
#                                      box is set up by too), and its server started
#   deploy/sprites/box.sh status NAME  its URL, raea, the server's health
#   deploy/sprites/box.sh token NAME   its RAEA_CLOUD_TOKEN, for the TypeScript client
#   deploy/sprites/box.sh key NAME     a raea agent key (repos and storage scopes) from stdin, or the clipboard, onto
#                                      the box as RAEA_AGENT_KEY: its agents clone raea projects (raea-repo clone OWNER
#                                      PROJECT DIR, deploy/git/repo.sh), push, and send renders' media by hash (CR22, CR23)
#
# RAEA_VERSION (0.1.6) and AGENTS ("kimi-code claude-acp") pick what goes on, and RAEA_CLOUD_REF takes sdk/cloud from
# raea's public repo at that commit instead of this working tree; the farm comes from
# ~/.config/raea-cloud/farm.env (RAEA_CLOUD_GPU_URL, RAEA_CLOUD_GPU_TOKEN). The Sprite's URL stays private to the org.
set -euo pipefail

# the org raea owns, never another business's (org.sh)
. "$(dirname "$0")/org.sh"
NAME=${2:-}
[ -n "$NAME" ] || { echo "box: name the Sprite"; exit 2; }
RAEA_VERSION=${RAEA_VERSION:-0.1.6}
AGENTS=${AGENTS:-kimi-code claude-acp}
HERE=$(cd "$(dirname "$0")/../.." && pwd)
FARM=${RAEA_FARM_ENV:-$HOME/.config/raea-cloud/farm.env}

# the Sprite's login shell warns about line editing on every exec; only the command's own status counts
quiet() { grep --line-buffered -v 'bind: warning' || true; }
x() { sprite exec -o "$ORG" -s "$NAME" -- bash -c "$*" 2>&1 | quiet; }
# a file onto the Sprite over exec's stdin (the CLI's own upload times out on large files)
put() { sprite exec -o "$ORG" -s "$NAME" -- bash -c "umask 077 && mkdir -p \"\$(dirname $2)\" && cat > $2" <"$1" 2>&1 | quiet; }

case "${1:-}" in
up)
  [ -f "$FARM" ] || { echo "box: $FARM names no farm (RAEA_CLOUD_GPU_URL, RAEA_CLOUD_GPU_TOKEN)"; exit 2; }
  sprite list -o "$ORG" | grep -qx "$NAME" || sprite create -o "$ORG" --skip-console --label raea --label raea-cloud "$NAME"
  # sdk/cloud's files from this working tree; with RAEA_CLOUD_REF set, setup.sh takes them from raea's public repo at
  # that commit instead, as a bot's box does
  if [ -z "${RAEA_CLOUD_REF:-}" ]; then
    pack=$(mktemp)
    COPYFILE_DISABLE=1 tar -czf "$pack" --no-mac-metadata -C "$HERE" server src image deploy/git package.json package-lock.json
    put "$pack" /tmp/raea-cloud.tgz
    rm -f "${pack:?}"
  fi
  put "$FARM" /tmp/farm.env
  # the box itself, set up by the one script a bot's box is set up by too
  if ! sprite exec -o "$ORG" -s "$NAME" -- bash -s -- "$NAME" "$RAEA_VERSION" "$AGENTS" "${RAEA_CLOUD_REF:-}" < "$(dirname "$0")/setup.sh" 2>&1 | quiet; then
    echo "box: setup.sh failed on $NAME"; exit 1
  fi
  "$0" status "$NAME"
  ;;
status)
  sprite url -o "$ORG" -s "$NAME" 2>&1 | quiet
  x "/opt/raea/bin/raea --version; . ~/.raea-cloud.env; curl -fsS -H \"authorization: Bearer \$RAEA_CLOUD_TOKEN\" localhost:8080/v1/health | head -c 600; echo"
  ;;
token) x ". ~/.raea-cloud.env; echo \$RAEA_CLOUD_TOKEN" ;;
key)
  # never echoed: from a pipe, or the clipboard when there's none
  if [ -t 0 ]; then k=$(pbpaste); else k=$(cat); fi
  k=$(printf %s "$k" | tr -d '[:space:]')
  case "$k" in raea_ak_*) ;; *) echo "box: that isn't a raea agent key (raea_ak_...)"; exit 2 ;; esac
  f=$(mktemp)
  (umask 077; printf 'RAEA_AGENT_KEY=%s\n' "$k" >"$f")
  put "$f" /tmp/agent.env
  rm -f "${f:?}"
  x "umask 077; grep -v '^RAEA_AGENT_KEY=' ~/.raea-cloud.env > ~/.raea-cloud.env.new; cat /tmp/agent.env >> ~/.raea-cloud.env.new; rm /tmp/agent.env; mv ~/.raea-cloud.env.new ~/.raea-cloud.env \
     && sprite-env services restart raea-cloud >/dev/null && echo 'box: agent key on, the server restarted'"
  ;;
*)
  sed -n '2,16p' "$0"
  exit 2
  ;;
esac
