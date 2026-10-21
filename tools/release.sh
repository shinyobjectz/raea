#!/bin/zsh
# A release of the raea engine for macOS on Apple Silicon: raea-motion and its bundler, with raea's own files beside
# them (share/raea: the @raea/motion SDK's TypeScript, the lite runtime and its packages, the shell's and browser's
# sources, the tools that install the local music and foley models, and the agent skills), laid out as
# tools/install.sh puts it under ~/.raea. Built from the committed tree at REF (never anyone's unfinished edit); the
# runtime's packages (sdk/react/js/node_modules, installed, not committed) come from this checkout.
#
#   tools/release.sh VERSION BIN_DIR [REF]     BIN_DIR holds a release build's raea-motion and libraea_bundle.dylib
#
# Writes dist/raea-VERSION-macos-arm64.tar.gz and its .sha256, and checks the tarball: unpacked into a fresh folder, its
# raea-motion renders a frame with RAEA_NO_TREE=1 (so it can't be reading this checkout). Nothing is published.
set -eu
version=${1:?usage: release.sh VERSION BIN_DIR [REF]}
bin=${2:?usage: release.sh VERSION BIN_DIR [REF]}
ref=${3:-HEAD}
root=${0:A:h}/..
cd $root
out=$root/dist
name=raea-$version-macos-arm64
stage=$(mktemp -d)/raea
mkdir -p $stage/bin $stage/share/raea $out
cp $bin/raea-motion $bin/libraea_bundle.dylib $stage/bin/
# the committed files raea-motion reads at run time
git archive $ref sdk/motion/src sdk/motion/library sdk/motion/package.json sdk/react/js/src sdk/react/js/lite sdk/react/js/shims sdk/react/js/package.json sdk/react/js/tsconfig.json engine/browser/ts sdk/shell/src \
  tools/tokensynth tools/sao sdk/skills/raea-bevy | tar -x -C $stage/share/raea
# the agent skill for raea itself lives with the public repository's files
mkdir -p $stage/share/raea/skills/raea
git show ${ref}:tools/publish/public/sdk/skills/raea/SKILL.md > $stage/share/raea/skills/raea/SKILL.md
# the runtime's packages, as installed here
rsync -a --exclude .cache sdk/react/js/node_modules $stage/share/raea/react/js/
rm -rf $stage/share/raea/tools/tokensynth/__pycache__
print -r -- "$version" > $stage/share/raea/VERSION
tar -C ${stage:h} -czf $out/$name.tar.gz raea
(cd $out && shasum -a 256 $name.tar.gz > $name.tar.gz.sha256)
# the check: unpacked elsewhere, it renders a frame without the tree it was built from
check=$(mktemp -d)
tar -xzf $out/$name.tar.gz -C $check
cat > $check/hello.tsx <<'TSX'
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";
function Hello() {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{ background: "#000", color: "#fff", fontSize: 120, justifyContent: "center", alignItems: "center" }}>raea {f}</AbsoluteFill>;
}
registerRoot(() => <Composition id="hello" component={Hello} width={640} height={360} fps={30} durationInFrames={30} />);
TSX
(cd $check && RAEA_NO_TREE=1 ./raea/bin/raea-motion render hello.tsx --frame 12 -o hello.png >/dev/null 2>&1) || { echo "release: the unpacked raea-motion didn't render (see $check)"; exit 1; }
[ -s $check/hello.png ] || { echo "release: no frame written (see $check)"; exit 1; }
echo "release: $out/$name.tar.gz ($(du -h $out/$name.tar.gz | cut -f1)), checked: an unpacked copy rendered a frame with RAEA_NO_TREE=1"
rm -rf $check ${stage:h}
