#!/bin/bash
# PT8, the Linux check after landing (docs/ports.feature): `cargo check` of raea-cli, of raea-render without its
# default features, and of the app's window as the Linux app builds it (raea-page-host --features window, whose bevy
# brings winit's X11 and Wayland: raea-page-window alone has none, so winit refuses Linux. The window's links and
# clipboard were Mac-only stubs off the Mac until 11-23, so signing in opened nothing on Linux), for x86_64 Linux on
# the probe box (Fly Sprite raea-linux in raea's own org, set up by tools/linux-box.sh), so an Apple-only call outside
# a cfg is caught the day it lands.
#
#   tools/linux-check.sh [SHA]                            # default: origin/main
#   tools/linux-check.sh --release VERSION [SHA] [--dry]  # RL3: the Linux tarball, built and tried there
#
# --release builds raea as the macOS release does, `raea-cli --bin raea --no-default-features` with the plugin catalog's
# features (tools/bevy-plugins.sh features sdk/raea/bench/bevy-plugins), with the dist profile (--dry: LTO off, debug 0)
# in the probe box's ubuntu:24.04 root, /srv/noble (debootstrapped with the build's -dev packages and rustup; target
# /srv/noble-target), so the tarball needs glibc 2.39, not the probe box's own 2.43; the app targets' two hosts too,
# raea-rune-host and raea-page-host's window build, in a cargo call each (docs/targets.feature T1f). Then tools/release.sh VERSION,
# and RL3's three steps on the unpacked tarball (raea init, a render of the template as a frame and as MP4, raea play
# of the bench's pong offscreen) in a private network and mount namespace: no network but loopback, ~/raea, the
# worktree and ~/.raea under empty tmpfs, env -i with an empty HOME and raea's API at a closed port. The tarball stays
# in the worktree's dist/ on the probe box.
#
# - A Mac cross check would need a Linux sysroot: twelve -sys crates' build scripts compile C (alsa, udev, fontconfig,
#   sqlite, openh264, opus, oniguruma, ring, psm, rquickjs, wasmtime, raea-runtime-react's build.rs). So it is an
#   after-land check on the probe box, not an L1 gate; a failure goes back to the lane that landed the commit.
# - The commit goes over as a git bundle on `sprite exec` stdin (the probe box can't reach the private repo), cut
#   against the commits the probe box already has. It is checked out in its own worktree (~/linux-check, target
#   ~/linux-check-target, kept warm), never in the probe box's ~/raea.
# - Output: one line a step ("linux <step>: ..."), the first errors, then "linux <sha>: ok" (exit 0), "FAILED" (exit 1)
#   or, when the connection to the probe box dropped first, "lost the probe box" (exit 2, run it again).
#   Run it under antsy with a stall of 20 minutes (a cold target builds every build script before printing).
# - Memory: the probe box has 15 GB and runs nothing else, and running it out of memory restarts the whole box (10-29,
#   on the 8 GB box before it: the release build at 7 jobs with thin LTO did). Every cargo here runs under a floor:
#   its process group is killed once the memory left falls below RAEA_LINUX_FLOOR_MB (default 1536), so the worst case
#   is a failed build ("stopped at N MB"), never a restarted box. Memory left is MemTotal less the root cgroup's anon
#   and kernel memory (memory.stat): the box's /proc/meminfo is synthetic (MemFree 0, MemAvailable equal to Cached and
#   past MemTotal), so MemAvailable never falls. The box allows no cgroup memory limit (its root
#   cgroup's subtree_control refuses +memory) and has no systemd, and ulimit -v caps each rustc's address space, which
#   LLVM reserves far past what it uses, not the sum. The release build runs 4 jobs, a dry run 7.
set -uo pipefail
cd "$(dirname "$0")/.."
# the org raea owns, never another business's (org.sh); RAEA_LINUX_BOX picks the Sprite
. sdk/cloud/deploy/sprites/org.sh
S=(-o "$ORG" -s "${RAEA_LINUX_BOX:-raea-linux}")
say() { echo "linux $*"; }
git fetch -q origin 2> /dev/null
mode=check version= dry=
if [ "${1:-}" = --release ]; then
  mode=release; version=${2:?usage: linux-check.sh --release VERSION [SHA] [--dry]}; shift 2
  [ "${2:-${1:-}}" = --dry ] && dry=1
  [ "${1:-}" = --dry ] && set -- "${@:2}"
fi
SHA=$(git rev-parse --short=9 "${1:-origin/main}^{commit}") || exit 2
REF=refs/linux-check/$SHA

# what the probe box has: every commit its refs point at that this repo knows, as the bundle's basis
basis=()
for c in $(sprite exec --no-stdin "${S[@]}" -- bash -c 'git -C ~/raea for-each-ref --format="%(objectname)"' 2> /dev/null | sort -u); do
  git cat-file -e "$c^{commit}" 2> /dev/null && basis+=("^$c")
done
FULL=$(git rev-parse "$SHA")
if sprite exec --no-stdin "${S[@]}" -- bash -c "git -C ~/raea cat-file -e $FULL^{commit} && git -C ~/raea update-ref $REF $FULL" 2> /dev/null; then
  say "sent: $SHA (the probe box had it)"
else
  git update-ref "$REF" "$SHA" || exit 2
  git bundle create -q - "$REF" ${basis[@]+"${basis[@]}"} \
    | sprite exec "${S[@]}" -- bash -c "cat > ~/linux-check.bundle && git -C ~/raea fetch -q ~/linux-check.bundle '+$REF:$REF' && rm ~/linux-check.bundle"
  sent=$?
  git update-ref -d "$REF"
  [ $sent = 0 ] || { say "$SHA: the bundle didn't reach the probe box"; exit 2; }
  say "sent: $SHA (${#basis[@]} commits as the basis)"
fi

# The check runs detached on the probe box and this session follows its log: cargo killed mid-write by a dropped
# connection leaves invalid metadata (E0786) in the target. Run again after a drop and it attaches to the same check.
cat <<'EOF' | sprite exec "${S[@]}" -- bash -c 'cat > ~/linux-check-run.sh' || { say "$SHA: couldn't reach the probe box"; exit 2; }
set -uo pipefail
W=~/linux-check
[ -d $W ] || git -C ~/raea worktree add -q --detach $W "$1" || exit 2
git -C $W checkout -q --detach "$1" && git -C ~/raea update-ref -d "$1" || exit 2
echo "linux checkout: $(git -C $W log -1 --format='%h %s' | cut -c1-90)"
ok=0
# CMD... in its own process group, killed whole when the box's available memory falls below the floor (see the header).
# Its group isn't the runner's, so stopping the runner stops it too (canary.2's cargo outlived a stopped runner).
GUARDED=
trap '[ -n "$GUARDED" ] && sudo kill -KILL -- -$GUARDED 2> /dev/null; exit 143' TERM INT HUP
guarded() {
  local floor=${RAEA_LINUX_FLOOR_MB:-1536} low= avail
  setsid "$@" &
  local pg=$!
  GUARDED=$pg
  while [ -d /proc/$pg ]; do
    avail=$(( $(awk '/^MemTotal:/ {print $2}' /proc/meminfo) / 1024 - $(awk '$1 == "anon" || $1 == "kernel" {s += $2} END {print int(s / 1048576)}' /sys/fs/cgroup/memory.stat) ))
    if [ "$avail" -lt "$floor" ]; then
      low=$avail; sudo kill -KILL -- -$pg; break
    fi
    sleep 0.5
  done
  wait $pg; local code=$?
  [ -z "$low" ] || { echo "linux: stopped at $low MB of memory available (floor $floor MB)"; return 1; }
  return $code
}
if [ "$2" = release ]; then
  V=${3:?version}; N=/srv/noble; T=/srv/noble-target; R=/tmp/linux-release
  free=$(df -BG --output=avail / | tail -1 | tr -dc 0-9)
  [ "$free" -ge 15 ] || { echo "linux refused: ${free} GB free (needs 15)"; echo "linux done 2"; exit 2; }
  # the runtime's packages, which release.sh copies in
  for d in sdk/react/js sdk/react/tw; do (cd "$W/$d" && bun install --frozen-lockfile 2>&1 | tail -1); done
  sudo mkdir -p "$N/.sprite" "$N/rel" "$N$HOME/raea"
  mountpoint -q "$N/.sprite" || sudo mount --bind -o ro /.sprite "$N/.sprite"
  for d in proc sys dev; do mountpoint -q "$N/$d" || sudo mount --bind "/$d" "$N/$d"; done
  # /rel is this worktree every time (an earlier run's mount may name another), and its .git's ~/raea resolves too
  mountpoint -q "$N/rel" && sudo umount "$N/rel"
  sudo mount --bind "$W" "$N/rel" || exit 2
  mountpoint -q "$N$HOME/raea" || sudo mount --bind -o ro "$HOME/raea" "$N$HOME/raea"
  mountpoint -q "$N/target" || sudo mount --bind "$T" "$N/target"
  slim=; [ -n "${4:-}" ] && slim="CARGO_PROFILE_DIST_LTO=off CARGO_PROFILE_DIST_DEBUG=0 CARGO_PROFILE_DIST_INCREMENTAL=false"
  # the first probe box had 8 GB: 7 jobs with thin LTO ran it out of memory and restarted it (10-29, canary.2's first try)
  jobs=7; [ -z "$slim" ] && jobs=4
  # RAEA_LINUX_DIST_ENV: profile overrides for a release build the box's memory can't hold as the dist profile has it
  # (canary.2: CARGO_PROFILE_DIST_DEBUG=0, since debug info is most of the final link's memory; the symbol table stays,
  # so the unstripped binary still names functions), and RAEA_LINUX_JOBS its job count
  [ -z "$slim" ] && slim=${RAEA_LINUX_DIST_ENV:-}; [ -n "${RAEA_LINUX_JOBS:-}" ] && jobs=$RAEA_LINUX_JOBS
  # the plugin catalog, as the macOS release has it (all 57 check on Linux, 10-29)
  feat=$(cd "$W" && bash tools/bevy-plugins.sh features sdk/raea/bench/bevy-plugins) && [ -n "$feat" ] || { echo "linux: no catalog features"; echo "linux done 1"; exit 1; }
  echo "linux catalog: $(tr , '\n' <<< "$feat" | wc -l) features"
  # the pinned toolchain's own bin first: without rustup's proxies (/root/.cargo/bin, gone by 11-06) cargo was the
  # probe box's, which doesn't run in the root
  tc=/root/.rustup/toolchains/$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$W/rust-toolchain.toml")-x86_64-unknown-linux-gnu/bin
  sudo test -x "$N$tc/cargo" || { echo "linux: no cargo at $N$tc"; echo "linux done 1"; exit 1; }
  # the probe box's resolver, so cargo can fetch a git dependency (11-06: rav1d's github.com didn't resolve in the root)
  sudo cp --remove-destination /etc/resolv.conf "$N/etc/resolv.conf"  # its own is a dangling link
  guarded sudo chroot "$N" bash -c "export PATH=$tc:/root/.cargo/bin:/.sprite/bin:\$PATH CARGO_TARGET_DIR=/target $slim
    git config --global --add safe.directory /rel; git config --global --add safe.directory $HOME/raea
    cd /rel && cargo build --profile dist --locked -j $jobs -p raea-cli --bin raea --no-default-features --features '$feat' 2>&1 | tee /target/dist-build.log | grep --line-buffered -E '^ *Compiling|^error|Finished'"
  grep -q '^ *Finished' "$T/dist-build.log" || { grep -E '^error' -A8 "$T/dist-build.log" | head -40; tail -3 "$T/dist-build.log"; echo "linux done 1"; exit 1; }
  # the app targets' hosts (docs/targets.feature T1f), each in a cargo call of its own: with raea-cli's crates in one
  # call taffy's features unify and rune-taffy doesn't build
  for h in raea-rune-host 'raea-page-host --features window'; do
    guarded sudo chroot "$N" bash -c "export PATH=$tc:/root/.cargo/bin:/.sprite/bin:\$PATH CARGO_TARGET_DIR=/target $slim
      cd /rel && cargo build --profile dist --locked -j $jobs -p $h 2>&1 | tee /target/host-build.log | grep --line-buffered -E '^error|Finished'"
    grep -q '^ *Finished' "$T/host-build.log" || { grep -E '^error' -A8 "$T/host-build.log" | head -40; tail -3 "$T/host-build.log"; echo "linux done 1"; exit 1; }
  done
  for b in raea raea-rune-host raea-page-host; do
    glibc=$(objdump -T "$T/dist/$b" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)
    echo "linux build: $b $glibc${4:+ (dry-run profile)}"
    [ "$(printf '%s\n' "$glibc" GLIBC_2.39 | sort -V | tail -1)" = GLIBC_2.39 ] || { echo "linux: $b needs $glibc, past 2.39"; echo "linux done 1"; exit 1; }
  done
  # the tarball's binaries stripped, from copies (the target's stay as cargo wrote them); the unstripped ones are kept
  # on the probe box for symbolicating a release's backtraces, never shipped
  ship=$T/ship; sym=$HOME/raea-dist-symbols/$V
  rm -rf "${ship:?}"; mkdir -p "$ship" "$sym"
  bins=("$T/dist/raea" "$T/dist/libraea_bundle.so" "$T/dist/raea-rune-host" "$T/dist/raea-page-host")
  sudo cp "${bins[@]}" "$sym/" && sudo cp "${bins[@]}" "$ship/" && sudo chown "$(id -un)" "$ship"/* "$sym"/* \
    && strip "$ship/raea" "$ship/raea-rune-host" "$ship/raea-page-host" && strip --strip-unneeded "$ship/libraea_bundle.so" \
    || { echo "linux: strip failed"; echo "linux done 1"; exit 1; }
  echo "linux strip: raea $(du -h "$sym/raea" | cut -f1) to $(du -h "$ship/raea" | cut -f1); unstripped in $sym"
  cd "$W" && zsh tools/release.sh "$V" "$ship" HEAD || { echo "linux done 1"; exit 1; }
  tgz=$W/dist/raea-$V-linux-x86_64.tar.gz
  [ -s "${tgz:?}" ] || { echo "linux: no tarball"; echo "linux done 1"; exit 1; }
  rm -rf "${R:?}"; mkdir -p "$R/home" "$R/p" "$R/game"
  tar -xzf "$tgz" -C "$R" && cp -R "$W/sdk/react/bench/games/pong/." "$R/game/"
  clean() {  # DIR ARGS...: raea in DIR as a fresh user
    local dir=$1; shift
    sudo unshare -n -m --propagation private bash -c '
      ip link set lo up 2> /dev/null
      for d in "$1/raea" "$1/linux-check" "$1/.raea"; do [ -d "$d" ] && mount -t tmpfs none "$d"; done
      r=$3; cd "$2" && shift 3 && exec sudo -u "$SUDO_USER" env -i PATH=/usr/bin:/bin HOME="$r/home" RAEA_NO_TREE=1 RAEA_API=http://127.0.0.1:9 RAEA_GEN_API=http://127.0.0.1:9 "$r/raea/bin/raea" "$@"' -- "$HOME" "$dir" "$R" "$@"
  }
  step() {  # NAME DIR ARGS...
    local n=$1 t0; shift; t0=$(date +%s)
    clean "$@" > "$R/$n.log" 2>&1; local code=$?
    echo "linux $n: exit $code, $(( $(date +%s) - t0 ))s"; grep -v -E 'ALSA|^\s*$' "$R/$n.log" | tail -2 | cut -c1-160
    [ $code = 0 ] || ok=1
  }
  step init "$R/p" init
  step render-frame "$R/p" render edit.tsx --frame 15 -o card.png
  step render-mp4 "$R/p" render edit.tsx -o edit.mp4
  step play "$R/game" play edit.tsx --offscreen --seconds 2 --size 540x960 --snapshot 1 snap.png
  # a raea/bevy program on the catalog: a headless App with avian and bevy, stepped, printing its frame
  # (raea run calls the script's default export; print is raea's stdout)
  mkdir -p "$R/bevy" && printf '%s\n' 'import { BevyApp } from "raea/bevy";' 'import { print } from "raea/native";' 'export default function main() {' \
    '  const app = BevyApp.open({ plugins: ["avian", "bevy"] });' '  const frame = app.step(30);' '  app.close();' \
    '  if (!(frame >= 30)) throw new Error(`stepped to ${frame}`);' '  print(`bevy ok: frame ${frame}`);' '}' > "$R/bevy/check.ts"
  step bevy "$R/bevy" run check.ts
  grep -q '^bevy ok' "$R/bevy.log" || { echo "linux: the BevyApp program didn't finish"; ok=1; }
  for f in "$R/p/card.png" "$R/p/edit.mp4" "$R/game/snap.png"; do [ -s "$f" ] || { echo "linux: no $f"; ok=1; }; done
  echo "linux tarball: $tgz ($(du -h "$tgz" | cut -f1))"
  echo "linux done $ok"; exit $ok
fi
cd $W && export CARGO_TARGET_DIR=~/linux-check-target
for args in '-p raea-cli' '-p raea-render --no-default-features' '-p raea-page-host --features window'; do
  t0=$(date +%s)
  guarded bash -c "cargo check $args -j 7 --message-format short > ~/linux-check.out 2>&1"
  code=$?
  grep -E '^(error|[^ ]+:[0-9]+:[0-9]+: error)' ~/linux-check.out | head -20 > ~/linux-check.err
  echo "linux cargo check $args: exit $code, $(( $(date +%s) - t0 ))s"
  cat ~/linux-check.err
  [ $code = 0 ] || ok=1
done
echo "linux done $ok"
EOF
sprite exec --no-stdin "${S[@]}" -- bash -lc "
  P=\$(cat ~/linux-check.pid 2> /dev/null)
  if [ -n \"\$P\" ] && kill -0 \$P 2> /dev/null; then
    [ \"\$(cat ~/linux-check.sha)\" = $SHA ] || { echo \"linux busy: checking \$(cat ~/linux-check.sha)\"; exit 2; }
    echo 'linux attached to the running check'
  else
    echo $SHA > ~/linux-check.sha
    # from a subshell, so init adopts and reaps the runner: as this shell's child it would stay a zombie under tail,
    # and tail --pid would never see it end
    ( RAEA_LINUX_DIST_ENV='${RAEA_LINUX_DIST_ENV:-}' RAEA_LINUX_JOBS='${RAEA_LINUX_JOBS:-}' setsid nohup bash ~/linux-check-run.sh '$REF' $mode '$version' '$dry' > ~/linux-check.log 2>&1 < /dev/null & echo \$! > ~/linux-check.pid )
  fi
  tail -n +1 --pid=\$(cat ~/linux-check.pid) -f ~/linux-check.log
" 2> /dev/null | tee /dev/stderr | grep -q '^linux done 0$'
code=(${PIPESTATUS[0]} $?)
if [ "${code[1]}" = 0 ]; then say "$SHA: ok"; exit 0; fi
[ "${code[0]}" = 0 ] && { say "$SHA: FAILED"; exit 1; }
say "$SHA: lost the probe box before the check finished (sprite exec exit ${code[0]}); run it again"; exit 2
