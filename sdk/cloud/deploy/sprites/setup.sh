#!/bin/bash
# One raea cloud box, set up on the Fly Sprite it runs on. Two callers run this same file: deploy/sprites/box.sh up
# from a Mac, and raea cloud's POST /v1/bots (B1: apps/site/cloud harness/sprite.ts, through the Sprites API, at the
# commit it pins), so a box made by hand and a box a bot makes can't differ (docs/cloud-render.feature CR11).
# It only adds what's missing, so running it again is safe.
#
#   bash -s -- NAME RAEA_VERSION "AGENTS" [REF] < setup.sh
#
# Before it runs, the caller writes the farm's env (RAEA_CLOUD_GPU_URL, RAEA_CLOUD_GPU_TOKEN) to /tmp/farm.env, and
# from a working tree, sdk/cloud's files to /tmp/raea-cloud.tgz. With no tarball there, it takes them from raea's
# public repo at REF. What goes on: raea's Linux release, Node, the ACP agents named in AGENTS with raea's skills for
# each, and sdk/cloud's server as the Sprite's HTTP service on 8080, `raea render` there sent to the farm.
set -euo pipefail
NAME=${1:?name} VERSION=${2:?raea version} AGENTS=${3:?agents} REF=${4:-}
NODE=24.22.0
say() { echo "box $NAME: $*"; }
export DEBIAN_FRONTEND=noninteractive

say packages
# raea's runtime libraries, and Mesa's lavapipe: the box draws previews on its CPU, its renders go to the farm
sudo -E apt-get update -qq
sudo -E apt-get install -y -qq --no-install-recommends ca-certificates curl git git-lfs zstd xz-utils procps ripgrep \
  libasound2t64 libfontconfig1 libvulkan1 mesa-vulkan-drivers vulkan-tools > /dev/null
node -v 2> /dev/null | grep -q '^v24' || {
  say "node $NODE"
  curl -fsSL "https://nodejs.org/dist/v$NODE/node-v$NODE-linux-x64.tar.xz" | sudo tar -xJ -C /opt
  sudo ln -sf /opt/node-v$NODE-linux-x64/bin/{node,npm,npx} /usr/local/bin/
}
if [ "$(/opt/raea/bin/raea --version 2> /dev/null)" != "$VERSION" ]; then
  say "raea $VERSION"
  f=raea-$VERSION-linux-x86_64.tar.gz u=https://github.com/shinyobjectz/raea/releases/download/v$VERSION
  (cd /tmp && curl -fsSL -o "$f" "$u/$f" && curl -fsSL -o "$f.sha256" "$u/$f.sha256" && sha256sum -c --quiet "$f.sha256")
  sudo rm -rf /opt/raea && sudo mkdir -p /opt/raea && sudo tar -xzf "/tmp/$f" -C /opt/raea --strip-components=1
  rm -f "/tmp/$f" "/tmp/$f.sha256"
fi

# sdk/cloud's server, client and wrapper, as the image has them (image/Dockerfile)
if [ ! -s /tmp/raea-cloud.tgz ]; then
  [ -n "$REF" ] || { echo "box: no /tmp/raea-cloud.tgz and no REF to take sdk/cloud from"; exit 2; }
  say "sdk/cloud at $REF"
  g=$(mktemp -d)
  git -C "$g" init -q && git -C "$g" sparse-checkout set sdk/cloud
  git -C "$g" fetch -q --depth 1 --filter=blob:none https://github.com/shinyobjectz/raea "$REF" && git -C "$g" checkout -q FETCH_HEAD
  tar -czf /tmp/raea-cloud.tgz -C "$g/sdk/cloud" server src image deploy/git package.json package-lock.json
  rm -rf "${g:?}"
fi
say server
sudo mkdir -p /opt/raea-cloud && sudo chown "$(id -u):$(id -g)" /opt/raea-cloud
tar -xzf /tmp/raea-cloud.tgz -C /opt/raea-cloud && rm /tmp/raea-cloud.tgz
cd /opt/raea-cloud
npm ci --omit=dev --no-fund --no-audit --loglevel=error
node server/agents.ts install $AGENTS
chmod +x image/*.sh deploy/git/*
sudo ln -sf /opt/raea-cloud/image/raea.sh /usr/local/bin/raea
sudo ln -sf /opt/raea-cloud/deploy/git/repo.sh /usr/local/bin/raea-repo
git lfs install --skip-repo > /dev/null
# the box's commits say which box made them
git config --global user.name "raea cloud $NAME"
git config --global user.email "$NAME@box.raea.io"
git config --global init.defaultBranch main

# the box's own token, made once and kept on its disk; the farm's, from the caller
umask 077
[ -s ~/.raea-cloud.env ] || echo "RAEA_CLOUD_TOKEN=$(openssl rand -hex 32)" > ~/.raea-cloud.env
if [ -s /tmp/farm.env ]; then
  { grep -v '^RAEA_CLOUD_GPU_' ~/.raea-cloud.env; cat /tmp/farm.env; } > ~/.raea-cloud.env.new
  mv ~/.raea-cloud.env.new ~/.raea-cloud.env && rm /tmp/farm.env
fi
grep -q '^RAEA_CLOUD_GPU_URL=' ~/.raea-cloud.env || { echo "box: no farm (write RAEA_CLOUD_GPU_URL and RAEA_CLOUD_GPU_TOKEN to /tmp/farm.env)"; exit 2; }
umask 022
for a in $AGENTS; do raea skills install --agent "$a" > /dev/null 2>&1 || say "raea skills for $a not installed"; done

# the server, as the Sprite's HTTP service on 8080: raea on lavapipe (docs/cloud.feature CL13), renders to the farm
cat > /opt/raea-cloud/image/sprite-start.sh << 'EOF'
#!/bin/bash
set -a
. ~/.raea-cloud.env
set +a
export PORT=8080 RAEA_CLOUD_DEVICE=cpu WGPU_BACKEND=vulkan __EGL_VENDOR_LIBRARY_DIRS=/nonexistent
export VK_DRIVER_FILES=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json | head -1)
mkdir -p ~/work
exec node /opt/raea-cloud/server/main.ts
EOF
chmod +x /opt/raea-cloud/image/sprite-start.sh
if sprite-env services list | grep -q '"raea-cloud"'; then
  sprite-env services restart raea-cloud > /dev/null
else
  sprite-env services create raea-cloud --cmd /opt/raea-cloud/image/sprite-start.sh --http-port 8080 > /dev/null
fi
say ready
