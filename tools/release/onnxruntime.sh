#!/usr/bin/env bash
# Windows x64's ONNX Runtime as Microsoft builds it (the Microsoft.ML.OnnxRuntime package on NuGet, MIT): onnxruntime.dll,
# its LICENSE and its ThirdPartyNotices.txt, put in DEST beside raea.exe for tools/release.sh to pack.
#
# raea.exe loads it when a model first runs (ort's load-dynamic on Windows x64, #110). It used to link the static runtime
# ort downloads, which is built for AVX2: its static initializers ran before main, so raea.exe stopped at start on every
# CPU without AVX2 and BMI2 (an Ivy Bridge, a Celeron), whatever was asked of it. Microsoft's runtime picks its kernels
# by the CPU it finds; it loads and opens raea's models under Intel's emulator as an Ivy Bridge and a Sandy Bridge.
# 1.28.0 is the runtime the static build was (ort-sys 2.0.0-rc.13's ms@1.28.0); ort asks for API 17 or newer.
# RAEA_WIN_ARCH=arm64 takes the package's win-arm64 runtime instead, for raea's Windows on ARM build (#82).
#   tools/release/onnxruntime.sh DEST
set -euo pipefail
die() { echo "onnxruntime: $*" >&2; exit 1; }
dest=${1:?DEST}
arch=${RAEA_WIN_ARCH:-x64}
case $arch in x64|arm64) ;; *) die "RAEA_WIN_ARCH is $arch: x64 or arm64" ;; esac
v=1.28.0
# the package's sha256; its sha512 is the one NuGet records for it (packageHash)
sum=769d1d3ea8ab6cd69f737c9dd4d4462aa4ad0ccfa106eaf506efc40d7bead5db
url=https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/$v/microsoft.ml.onnxruntime.$v.nupkg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 5 --retry-all-errors --connect-timeout 30 -o "$tmp/ort.nupkg" "$url" || die "couldn't download $url"
got=$( (sha256sum "$tmp/ort.nupkg" 2>/dev/null || shasum -a 256 "$tmp/ort.nupkg") | cut -d' ' -f1)
[ "$got" = "$sum" ] || die "Microsoft.ML.OnnxRuntime $v is $got, not the $sum pinned here"
unzip -q -o "$tmp/ort.nupkg" runtimes/win-$arch/native/onnxruntime.dll LICENSE ThirdPartyNotices.txt -d "$tmp/x"
mkdir -p "$dest"
cp "$tmp/x/runtimes/win-$arch/native/onnxruntime.dll" "$dest/onnxruntime.dll"
cp "$tmp/x/LICENSE" "$dest/onnxruntime-LICENSE.txt"
cp "$tmp/x/ThirdPartyNotices.txt" "$dest/onnxruntime-ThirdPartyNotices.txt"
echo "onnxruntime: Microsoft.ML.OnnxRuntime $v's win-$arch onnxruntime.dll ($(wc -c < "$dest/onnxruntime.dll" | tr -d ' ') bytes) and its notices in $dest"
