#!/usr/bin/env bash
# What raea.exe needs beside it on Windows, so a PC with nothing installed and no way to download (a work PC whose
# IT blocks GitHub's downloads) starts raea, reads a phone's video and sets up a local model. tools/release.sh packs it
# into the Windows zip's bin\, and the Windows app takes that bin\ whole.
#   - the Visual C++ runtime: vcruntime140.dll, vcruntime140_1.dll, msvcp140.dll and msvcp140_1.dll, which raea.exe and
#     onnxruntime.dll import. Microsoft's redistributable, from the Visual Studio on the runner that built raea, put
#     beside the exe as Microsoft documents for app-local deployment; until 0.1.8 a PC without the VC++ redistributable
#     installed couldn't start raea.exe at all
#   - FFmpeg (ffmpeg.exe, ffprobe.exe): raea's own LGPL build (tools/release/ffmpeg-win.sh), the one setup.rs fetches
#     where it isn't, with its licence and where its source is
#   - uv (uv.exe), the release every local model's setup was measured with (MIT or Apache 2.0), with both licences
#   - DXC (dxcompiler.dll, dxil.dll), Microsoft's shader compiler, from its release v1.8.2502 (wgpu's minimum), with its
#     three licences: wgpu compiles DX12's shaders with it when it sits beside raea.exe, else with the old FXC, which cost
#     6 to 13 s of hitches at start and as new 3D appeared on every PC DX12 is chosen for: Intel, AMD and Arc (#215; the
#     chat page 17.7 s with FXC, 12.0 s with DXC on the test PC)
# The pins are setup.rs's, so what ships and what's fetched are the same build.
#   tools/release/win-bundle.sh DEST          (on the Windows runner, in Git's bash)
set -euo pipefail
die() { echo "win-bundle: $*" >&2; exit 1; }
dest=${1:?DEST}
root=$(cd "$(dirname "$0")/../.." && pwd)
setup=$root/io/assets/src/setup.rs
const() { sed -n "s/^const $1: &str = \"\\(.*\\)\";$/\\1/p" "$setup"; }
sha() { (sha256sum "$1" 2>/dev/null || shasum -a 256 "$1") | cut -d' ' -f1; }
fetch() { curl -fsSL --retry 5 --retry-all-errors --connect-timeout 30 -o "$2" "$1" || die "couldn't download $1"; [ "$(sha "$2")" = "$3" ] || die "$1 isn't the $3 pinned"; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$dest"

# the VC++ runtime, from the newest Visual Studio with the C++ tools
vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
[ -x "$vswhere" ] || die "no vswhere at $vswhere: the C runtime comes from the runner's Visual Studio"
vs=$(cygpath -u "$("$vswhere" -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath | tr -d '\r')")
ver=$(tr -d '\r\n ' < "$vs/VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt")
crt=$(ls -d "$vs/VC/Redist/MSVC/$ver/x64"/Microsoft.VC14*.CRT 2>/dev/null | tail -1)
[ -n "$crt" ] || die "no Microsoft.VC14*.CRT under $vs/VC/Redist/MSVC/$ver/x64"
for f in vcruntime140.dll vcruntime140_1.dll msvcp140.dll msvcp140_1.dll; do
  [ -f "$crt/$f" ] || die "$crt has no $f"
  cp "$crt/$f" "$dest/$f"
done
printf '%s\r\n' "vcruntime140.dll, vcruntime140_1.dll, msvcp140.dll and msvcp140_1.dll are Microsoft's Visual C++ runtime ($ver)," \
  "redistributed with raea as Microsoft's Visual Studio licence terms allow, so raea starts on a PC that doesn't have the" \
  "Visual C++ Redistributable installed. They're Microsoft's, not raea's." > "$dest/vcruntime-NOTICE.txt"

# FFmpeg
url=$(const FFMPEG_URL); sum=$(const FFMPEG_SUM); top=$(const FFMPEG_TOP)
[ -n "$url" ] && [ -n "$sum" ] && [ -n "$top" ] || die "no FFMPEG_URL, FFMPEG_SUM and FFMPEG_TOP in $setup"
fetch "$url" "$tmp/ffmpeg.zip" "$sum"
unzip -q -o "$tmp/ffmpeg.zip" -d "$tmp/ff"
cp "$tmp/ff/$top/bin/ffmpeg.exe" "$tmp/ff/$top/bin/ffprobe.exe" "$dest/"
cp "$tmp/ff/$top/LICENSE" "$dest/ffmpeg-LICENSE.txt"
cp "$tmp/ff/$top/README.txt" "$dest/ffmpeg-README.txt"
cp "$tmp/ff/$top/libvpx-LICENSE" "$dest/libvpx-LICENSE.txt"
cp "$tmp/ff/$top/libvpx-PATENTS" "$dest/libvpx-PATENTS.txt"

# uv, and its licences from its source at the same tag
uv=$(const UV)
uvsum=$(sed -n 's/^ *("x86_64-pc-windows-msvc", "\([0-9a-f]*\)"),$/\1/p' "$setup")
[ -n "$uv" ] && [ -n "$uvsum" ] || die "no UV and its x86_64-pc-windows-msvc sha256 in $setup"
fetch "https://github.com/astral-sh/uv/releases/download/$uv/uv-x86_64-pc-windows-msvc.zip" "$tmp/uv.zip" "$uvsum"
unzip -q -o "$tmp/uv.zip" uv.exe -d "$dest"
[ "$uv" = 0.12.24 ] || die "uv is $uv: pin its LICENSE-MIT and LICENSE-APACHE here for that tag"
fetch "https://raw.githubusercontent.com/astral-sh/uv/$uv/LICENSE-MIT" "$dest/uv-LICENSE-MIT.txt" 860e3d7a86b84e6a7012c7a635fc64df475cebc6cce34dfeb73a5982ec58176c
fetch "https://raw.githubusercontent.com/astral-sh/uv/$uv/LICENSE-APACHE" "$dest/uv-LICENSE-APACHE.txt" c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4

# DXC: Microsoft's distributable dxcompiler.dll and dxil.dll (LICENSE-MS.txt), the x64 ones, and the release's licences
dxc=v1.8.2502
fetch "https://github.com/microsoft/DirectXShaderCompiler/releases/download/$dxc/dxc_2025_02_20.zip" "$tmp/dxc.zip" 70b1913a1bfce4a3e1a5311d16246f4ecdf3a3e613abec8aa529e57668426f85
unzip -q -o -j "$tmp/dxc.zip" bin/x64/dxcompiler.dll bin/x64/dxil.dll -d "$dest"
for l in MS LLVM MIT; do unzip -p "$tmp/dxc.zip" "LICENSE-$l.txt" > "$dest/dxc-LICENSE-$l.txt"; done

echo "win-bundle: in $dest: the VC++ runtime $ver, FFmpeg ($top), uv $uv, DXC $dxc, with their licences"
