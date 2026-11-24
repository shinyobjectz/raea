#!/bin/bash
# The Linux test machine tools/linux-check.sh runs on: Fly Sprite raea-linux, in the Sprites org raea owns
# (sdk/cloud/deploy/sprites/org.sh). Ubuntu 26.04 x86_64, 8 CPUs, 15 GB, Vulkan on Mesa's llvmpipe. It is a machine of
# its own, apart from the agent boxes (deploy/sprites/box.sh): a release build takes all of a Sprite's memory, and on
# 10-29 one restarted the Sprite it shared with an agent.
#
#   tools/linux-box.sh up       the Sprite, made or completed: the build packages, the toolchain rust-toolchain.toml
#                               pins, raea's public repo in ~/raea (linux-check sends newer commits as a bundle), and
#                               /srv/noble, the ubuntu 24.04 root a release is built in (so it needs glibc 2.39, not the
#                               Sprite's 2.43), with the same packages and the toolchain in its /root/.rustup
#   tools/linux-box.sh status   what it has
#
# RAEA_LINUX_BOX picks the Sprite (raea-linux). `up` only adds what's missing, so it is safe to run again.
set -euo pipefail
cd "$(dirname "$0")/.."
. sdk/cloud/deploy/sprites/org.sh
NAME=${RAEA_LINUX_BOX:-raea-linux}
CH=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
[ -n "$CH" ] || { echo "linux-box: no channel in rust-toolchain.toml"; exit 2; }
# the Sprite's login shell warns about line editing on every exec; only the command's own status counts
quiet() { grep --line-buffered -v 'bind: warning' || true; }
# a script on the Sprite, from stdin
run() { sprite exec -o "$ORG" -s "$NAME" -- bash -s -- "$@" 2>&1 | quiet; return "${PIPESTATUS[0]}"; }

case "${1:-}" in
up)
  sprite list -o "$ORG" | grep -qx "$NAME" || sprite create -o "$ORG" --skip-console --label raea --label linux-check "$NAME"
  run "$CH" <<'EOF'
set -euo pipefail
CH=$1 N=/srv/noble
# what a raea build compiles C against (alsa, udev, fontconfig, the windowing libraries, openh264's nasm), in both roots
PKGS="build-essential pkg-config cmake ninja-build nasm clang libclang-dev git git-lfs ca-certificates curl zstd binutils
  libasound2-dev libudev-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxi-dev libxcursor-dev
  libxrandr-dev libvulkan-dev libssl-dev libfontconfig-dev"
export DEBIAN_FRONTEND=noninteractive
echo "linux-box: packages"
sudo -E apt-get update -qq
sudo -E apt-get install -y -qq --no-install-recommends $PKGS debootstrap mesa-vulkan-drivers vulkan-tools ffmpeg zsh > /dev/null
echo "linux-box: toolchain $CH"
rustup toolchain install "$CH" --profile minimal > /dev/null 2>&1
[ -d ~/raea/.git ] || { echo "linux-box: raea's public repo"; GIT_LFS_SKIP_SMUDGE=1 git clone -q https://github.com/shinyobjectz/raea ~/raea; }
if ! sudo test -x "$N/root/.rustup/toolchains/$CH-x86_64-unknown-linux-gnu/bin/cargo"; then
  if ! sudo test -x "$N/usr/bin/apt-get"; then
    echo "linux-box: $N, ubuntu 24.04"
    sudo debootstrap --variant=minbase --components=main,universe noble "$N" http://archive.ubuntu.com/ubuntu > /dev/null
  fi
  for d in proc sys dev; do mountpoint -q "$N/$d" || sudo mount --bind "/$d" "$N/$d"; done
  sudo cp --remove-destination /etc/resolv.conf "$N/etc/resolv.conf"
  echo "linux-box: $N's packages"
  sudo chroot "$N" bash -c "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq --no-install-recommends $(echo $PKGS) > /dev/null"
  echo "linux-box: $N's toolchain $CH"
  sudo chroot "$N" bash -c "curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain $CH --no-modify-path > /dev/null"
fi
sudo mkdir -p /srv/noble-target
echo "linux-box: ready"
EOF
  "$0" status
  ;;
status)
  run "$CH" <<'EOF'
CH=$1 N=/srv/noble
echo "linux-box: $(. /etc/os-release; echo $PRETTY_NAME), $(nproc) CPUs, $(free -g | awk '/^Mem:/ {print $2}') GB, $(df -BG --output=avail / | tail -1 | tr -d ' ') free"
echo "linux-box: raea $(git -C ~/raea log -1 --format='%h %cs' 2>/dev/null || echo missing)"
echo "linux-box: toolchain $(rustup run "$CH" rustc --version 2>/dev/null || echo "$CH missing")"
v=$(sudo chroot "$N" ldd --version 2>/dev/null | head -1 | awk '{print $NF}')
c=$(sudo chroot "$N" /root/.rustup/toolchains/$CH-x86_64-unknown-linux-gnu/bin/cargo --version 2>/dev/null)
echo "linux-box: $N ${v:+glibc $v, }${c:-no cargo}"
EOF
  ;;
*)
  sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
  exit 2
  ;;
esac
