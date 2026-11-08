#!/bin/bash
# The check every release archive passes (tools/release.sh runs it on the archive it just made; release.yml's
# windows-verify job runs it again on a fresh Windows runner, from the archive alone): unpacked into a fresh folder,
# with RAEA_NO_TREE=1 so it can't be reading a checkout, its raea renders a frame, has the plugin catalog, prints a
# craft guide, writes styles.gen.ts and maps a song; its binaries name none of raea's API, nothing in it looks like a
# key, and a Windows archive holds no Microsoft file (the CRT comes from the VC++ redistributable;
# docs/handoff/ports.md).
#
#   tools/release-check.sh ARCHIVE     raea-VERSION-PLATFORM.tar.gz or .zip
set -eu
archive=${1:?usage: release-check.sh ARCHIVE}
# RAEA_CHECK_DIR keeps the check's folder where a CI job can upload it when a check fails (release.yml)
check=${RAEA_CHECK_DIR:-$(mktemp -d)}
mkdir -p "$check"
case $archive in
  *.zip) if command -v unzip >/dev/null; then unzip -q "$archive" -d "$check"; else 7z x -y -o"$check" "$archive" >/dev/null; fi ;;
  *) tar -xzf "$archive" -C "$check" ;;
esac
fail() { echo "release: $* (see $check)" >&2; exit 1; }
raea=$check/raea/bin/raea
[ -f "$raea.exe" ] && raea=$raea.exe
[ -f "$raea" ] || fail "no raea in bin/"
# the edit in a folder of its own: raea reads every file under a project, and the install isn't the user's project
mkdir -p "$check/p"
cat > "$check/p/hello.tsx" <<'TSX'
import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
function Hello() {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{ background: "#000", color: "#fff", fontSize: 120, justifyContent: "center", alignItems: "center" }}>raea {f}</AbsoluteFill>;
}
registerRoot(() => <Composition id="hello" component={Hello} width={640} height={360} fps={30} durationInFrames={30} />);
TSX
(cd "$check/p" && RAEA_NO_TREE=1 RUST_BACKTRACE=1 "$raea" render hello.tsx --frame 12 -o hello.png > render.log 2>&1) || { s=$?; tail -40 "$check/p/render.log" >&2; fail "the unpacked raea didn't render (exit $s)"; }
[ -s "$check/p/hello.png" ] || fail "no frame written"
# hello.tsx draws white text on black, so a frame raea finds all black lost its page (Windows' c:\ paths read as a URL
# scheme drew nothing but the black behind it, 11-05)
! grep -q "is all black" "$check/p/render.log" || { grep "all black" "$check/p/render.log" >&2; fail "the unpacked raea rendered a black frame"; }
# the plugin catalog is in it (track 3, raea/plugins): a headless App with avian and Bevy's input, raea/input's virtual
# gamepad pressed and read back. A raea built without --features "bevy-plugins,..." says "no plugin "avian" in this
# build" here
mkdir -p "$check/plugins"
cat > "$check/plugins/check.ts" <<'TS'
import { BevyApp } from "raea/bevy";
import { print } from "raea/native";
export default function main() {
  const app = BevyApp.open({ plugins: ["avian", "bevy"], fps: 60 });
  app.step(2);
  app.call("raea/input", { gamepad: "South" });
  app.step(2);
  const held = app.call<{ buttons: string[] }>("raea/input.gamepad");
  if (!held.buttons.includes("South")) throw new Error(`the virtual gamepad's South isn't held: ${JSON.stringify(held)}`);
  print("plugins: avian and the virtual gamepad");
}
TS
(cd "$check/plugins" && RAEA_NO_TREE=1 "$raea" run check.ts > run.log 2>&1; grep -q "plugins: avian and the virtual gamepad" run.log) || { tail -20 "$check/plugins/run.log" >&2; fail "the unpacked raea has no plugin catalog (build raea with --features \"\$(tools/bevy-plugins.sh features sdk/raea/bench/bevy-plugins)\")"; }
# and its guides and styles.gen come from the release too
(cd "$check/p" && RAEA_NO_TREE=1 "$raea" guide music | grep -q "music") || fail "the unpacked raea has no craft guides"
mkdir -p "$check/styled" && { echo 'import { css } from "./styles.gen";'; cat "$check/p/hello.tsx"; echo 'void css;'; } > "$check/styled/hello.tsx"
(cd "$check/styled" && RAEA_NO_TREE=1 "$raea" render hello.tsx --frame 1 -o hello.png > render.log 2>&1 && [ -s styles.gen.ts ]) \
  || { tail -20 "$check/styled/render.log" >&2; fail "the unpacked raea didn't write styles.gen.ts"; }
# the app targets (docs/targets.feature T1c, T1e, T1f): an app importing react, @raea/ui and @raea/rune, bundled by the
# release's own raea (no bun), checked headless on each target with one press: its frames, the press drawn (the
# frame after it differs from the mount's) and an empty unmapped.json; filmed on native, the public engine's MP4 says
# it's unsigned and carries no Content Credentials
mkdir -p "$check/app"
cat > "$check/app/app.tsx" <<'TSX'
import { useState } from "react";
import { emit } from "@raea/ui/wire";
import { Spring, rune } from "@raea/rune";
export default function App() {
  const [n, setN] = useState(0);
  const add = () => { setN(n + 1); emit("added", n + 1); };
  return (
    <div style={{ display: "flex", gap: 12, padding: 24, backgroundColor: "#111111", color: "#ffffff", fontSize: 24 }}>
      <button data-testid="add" onClick={add} style={{ padding: 8, backgroundColor: "#2563eb", color: "#ffffff", borderRadius: 8 }} {...rune({ motion: Spring(0.3) })}>Add</button>
      <span data-testid="count">{"Count: " + n}</span>
    </div>
  );
}
TSX
echo '[{"press": "add"}]' > "$check/app/steps.json"
for t in web native; do
  film=(); [ $t = native ] && film=(--video "$check/app/$t.mp4")
  (cd "$check/app" && RAEA_NO_TREE=1 "$raea" app check app.tsx --target $t --script steps.json --png "out-$t" --size 480x200 "${film[@]+"${film[@]}"}" > "$t.log" 2>&1) || { tail -20 "$check/app/$t.log" >&2; fail "raea app check --target $t failed"; }
  o=$check/app/out-$t
  [ -s "$o/00-mount.png" ] && [ -s "$o/01-press-add.png" ] || fail "raea app check --target $t wrote no frames"
  ! cmp -s "$o/00-mount.png" "$o/01-press-add.png" || fail "raea app check --target $t: the press didn't change the frame"
  [ "$(tr -d ' \n' < "$o/unmapped.json")" = "[]" ] || fail "raea app check --target $t: unmapped.json isn't empty: $(head -c 300 "$o/unmapped.json")"
done
grep -q "unsigned" "$check/app/native.log" || { tail -5 "$check/app/native.log" >&2; fail "the native film's report doesn't say it's unsigned"; }
! grep -qa c2pa "$check/app/native.mp4" || fail "the public engine's native film carries Content Credentials"
# raea song on every platform (#43): the raea-perceive beside raea maps a 12 s drum loop at 120 BPM (perl writes it:
# every runner has perl, Git's bash too) with the beat model the release ships, copied into a models folder of the
# check's own (HOME, and LOCALAPPDATA on Windows)
mkdir -p "$check/song/home"
perl - "$check/song/loop.wav" <<'PL'
my ($out) = @ARGV; my $sr = 22050; my $n = 12 * $sr; my @s; srand(1);
for my $i (0 .. $n - 1) {
  my $b = 2 * $i / $sr; my $ph = ($b - int($b)) / 2; my $h = 2 * $b; my $hp = ($h - int($h)) / 4;
  my $kick = sin(2 * 3.14159265 * (50 + 100 * exp(-$ph * 40)) * $ph) * exp(-$ph * 12) * (int($b) % 4 == 0 ? 0.9 : 0.6);
  push @s, int(32767 * 0.8 * ($kick + (rand() * 2 - 1) * exp(-$hp * 120) * 0.15));
}
open my $f, '>:raw', $out or die "$out: $!";
print $f 'RIFF', pack('V', 36 + 2 * $n), 'WAVEfmt ', pack('VvvVVvv', 16, 1, 1, $sr, 2 * $sr, 2, 16), 'data', pack('V', 2 * $n), pack('s<*', @s);
PL
(cd "$check/song" && HOME="$check/song/home" LOCALAPPDATA="$check/song/home" RAEA_NO_TREE=1 "$raea" song loop.wav > song.log 2>&1) || { tail -20 "$check/song/song.log" >&2; fail "the unpacked raea didn't map a song"; }
tempo=$(jq -r '.tempo' "$check/song/loop.song.json" | tr -d '\r')
awk -v t="$tempo" 'BEGIN { exit !(t >= 118 && t <= 122) }' || fail "raea song read the 120 BPM loop as $tempo BPM"
# engine only: no raea API in the binaries, nothing key-shaped anywhere
api=$(cat "$check"/raea/bin/* | grep -aoE '(api|sign)\.raea\.io|RAEA_(SIGN|GEN)_TOKEN' | sort -u | tr '\n' ' ' || true)
[ -z "$api" ] || fail "the binaries name raea's API ($api): build raea-cli with --no-default-features"
keys=$(grep -rlE 'sk-or-v1-[0-9a-f]{20}|sk-ant-[A-Za-z0-9_-]{20}|(^|[^A-Za-z0-9+/])AKIA[0-9A-Z]{16}([^A-Za-z0-9+/=]|$)|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY' "$check/raea" 2>/dev/null | head -5 || true)
[ -z "$keys" ] || fail "key-shaped text in: $keys"
# Windows: our binaries only. Any other DLL, or a file named like the CRT, the SDK or DirectX's redistributables, is
# Microsoft's and isn't ours to ship
ms=$(cd "$check/raea" && find . -type f \( -iname '*.dll' ! -path './bin/raea_bundle.dll' -o -iname 'vcruntime*' -o -iname 'msvcp*' -o -iname 'ucrtbase*' -o -iname 'api-ms-win*' -o -iname 'concrt*' -o -iname 'vccorlib*' -o -iname '*.lib' -o -iname '*.pdb' -o -iname 'd3dcompiler*' -o -iname 'dxcompiler*' -o -iname 'dxil*' \) | head -5)
[ -z "$ms" ] || fail "files that aren't ours: $ms"
echo "release: $archive checked: an unpacked copy rendered a frame and checked an app on both targets with RAEA_NO_TREE=1"
rm -rf "$check"
