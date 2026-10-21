#!/bin/zsh
# @raea/motion/codecs (mediabunny with the engine's coders) against `rdb ffmpeg` and real ffmpeg: the same jobs on the
# same file, compared. Jobs: probe, a still at 1.0 s, a trim 1..3 s (re-encoded), a 720p proxy (scaled natively),
# 16 kHz mono audio. Prints one line per comparison and exits 1 on any FAIL.
#   sdk/motion/tools/media-vs-ffmpeg.sh VIDEO.mp4    (RAEA_MOTION=raea-motion, RAEA_RDB=rdb)
set -u
here=${0:A:h}/..
in=${1:A}
M=${RAEA_MOTION:-$here/../react/target/release/raea-motion}
R=${RAEA_RDB:-$here/../rdb/target/release/rdb}
W=$(mktemp -d)
fails=0
ok() { echo "PASS $*"; }
bad() { echo "FAIL $*"; fails=$((fails+1)); }
cat > $W/job.ts <<TS
import { BufferTarget, Conversion, Mp4OutputFormat, Output, VideoSampleSink, WavOutputFormat } from "mediabunny";
import { fileInput, fileTarget, nativeScale, saveTarget, savePng, transcode, useNativeMedia } from "@raea/motion/codecs";
import { print } from "@raea/motion/native";

const IN = ${(qqq)in};
const OUT = ${(qqq)W};
const now = () => (globalThis as any).__raea_now() as number;

export default async function main() {
  useNativeMedia();
  const input = fileInput(IN);
  const v = await input.getPrimaryVideoTrack();
  const a = await input.getPrimaryAudioTrack();
  const stats = v ? await v.computePacketStats() : null;
  print("PROBE " + JSON.stringify({ duration: await input.computeDuration(), width: v?.codedWidth, height: v?.codedHeight, rotation: v?.rotation, codec: v?.codec, fps: stats?.averagePacketRate, frames: stats?.packetCount, audio: a ? { codec: a.codec, rate: a.sampleRate, channels: a.numberOfChannels } : null }));
  let t = now();
  if (v) {
    const sink = new VideoSampleSink(v);
    const s = await sink.getSample(1.0);
    if (s) { savePng(s, OUT + "/still.png"); s.close(); }
  }
  print("TIME still " + (now() - t).toFixed(0));
  if (v) {
    t = now();
    const out = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    const c = await Conversion.init({ input: fileInput(IN), output: out, trim: { start: 1, end: 3 }, video: { codec: "avc", forceTranscode: true } });
    await c.execute();
    saveTarget(out.target as BufferTarget, OUT + "/trim.mp4");
    print("TIME trim " + (now() - t).toFixed(0));
    t = now();
    const h = 720, w = Math.round((v.displayWidth * h) / v.displayHeight / 2) * 2;
    // mediabunny's own resize (and the track's rotation applied), done by the engine's transformer
    const out2 = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    const c2 = await Conversion.init({ input: fileInput(IN), output: out2, video: { codec: "avc", forceTranscode: true, width: w, height: h, fit: "fill" }, audio: { discard: true } });
    await c2.execute();
    saveTarget(out2.target as BufferTarget, OUT + "/proxy.mp4");
    print("TIME proxy " + (now() - t).toFixed(0));
    // the same jobs handed whole to the engine
    t = now();
    transcode(IN, OUT + "/heavy-proxy.mp4", { height: 720 });
    print("TIME proxy-native " + (now() - t).toFixed(0));
    transcode(IN, OUT + "/heavy-trim.mp4", { from: 1, to: 3 });
  }
  if (v) {
    // a remux: the same packets in an MP4, nothing re-encoded
    t = now();
    // written by the engine as it's muxed (a long take isn't held in JavaScript)
    const out4 = new Output({ format: new Mp4OutputFormat(), target: fileTarget(OUT + "/remux.mp4") });
    const c4 = await Conversion.init({ input: fileInput(IN), output: out4 });
    await c4.execute();
    print("TIME remux " + (now() - t).toFixed(0));
  }
  if (a) {
    // the sound at its own rate and channels
    const out5 = new Output({ format: new WavOutputFormat(), target: new BufferTarget() });
    const c5 = await Conversion.init({ input: fileInput(IN), output: out5, video: { discard: true }, audio: { codec: "pcm-s16" } });
    await c5.execute();
    saveTarget(out5.target as BufferTarget, OUT + "/anative.wav");
    t = now();
    const out3 = new Output({ format: new WavOutputFormat(), target: new BufferTarget() });
    const c3 = await Conversion.init({ input: fileInput(IN), output: out3, video: { discard: true }, audio: { sampleRate: 16000, numberOfChannels: 1, codec: "pcm-s16" } });
    await c3.execute();
    saveTarget(out3.target as BufferTarget, OUT + "/a16.wav");
    print("TIME audio " + (now() - t).toFixed(0));
  }
}
TS
$M run $W/job.ts > $W/log.txt 2>&1 || { echo "FAIL the mediabunny job: $(tail -5 $W/log.txt)"; exit 1; }
grep TIME $W/log.txt | sed 's/^/      /'
probe=$(grep '^PROBE ' $W/log.txt | cut -c7-)
pj() { python3 -I -c "import json,sys; v=json.loads(sys.argv[1]); print(eval(sys.argv[2]))" "$probe" "$1"; }
# probe against ffprobe and rdb ffprobe
for tool in ffprobe "$R ffprobe"; do
  d=$(${=tool} -v error -show_entries format=duration -of csv=p=0 $in)
  wh=$(${=tool} -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 $in | cut -d, -f1,2)
  python3 -I -c "import sys; sys.exit(abs($(pj "v['duration']") - $d) > 0.05)" && ok "probe duration $(pj "round(v['duration'],3)") (${${tool%% *}:t} $d)" || bad "probe duration $(pj "v['duration']") vs ${${tool%% *}:t} $d"
  [ "$(pj "f\"{v['width']},{v['height']}\"")" = "$wh" ] && ok "probe size $wh (${${tool%% *}:t})" || bad "probe size $(pj "f\"{v['width']},{v['height']}\"") vs ${${tool%% *}:t} $wh"
done
# the still: SSIM against ffmpeg's and rdb's
ffmpeg -v error -y -ss 1 -i $in -frames:v 1 $W/ff.png; $R ffmpeg -v error -y -ss 1 -i $in -frames:v 1 $W/rdb.png
for ref in ff rdb; do s=$(ffmpeg -i $W/still.png -i $W/$ref.png -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2); python3 -I -c "import sys; sys.exit(${s:-0} < 0.98)" && ok "still SSIM $s against $ref" || bad "still SSIM ${s:-none} against $ref"; done
# the trim: frames and length against ffmpeg's and rdb's trims; the pictures by SSIM
ffmpeg -v error -y -ss 1 -t 2 -i $in $W/ff-trim.mp4; $R ffmpeg -v error -y -ss 1 -t 2 -i $in $W/rdb-trim.mp4
# a file's video as it's shown: width,height (a 90/270 rotation swaps them),frames
info() { python3 -I - "$1" <<'PY'
import json, subprocess, sys
s = json.loads(subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=nb_frames,width,height:stream_side_data=rotation", "-of", "json", sys.argv[1]], capture_output=True, text=True).stdout)["streams"][0]
rot = int(abs(float(next((d.get("rotation", 0) for d in s.get("side_data_list", []) if "rotation" in d), 0)))) % 180
w, h = (s["height"], s["width"]) if rot == 90 else (s["width"], s["height"])
print(f"{w},{h},{s.get('nb_frames', '')}")
PY
}
for ref in ff-trim rdb-trim; do
  [ "$(info $W/trim.mp4 | cut -d, -f1,2)" = "$(info $W/$ref.mp4 | cut -d, -f1,2)" ] && ok "trim size $(info $W/trim.mp4) ($ref $(info $W/$ref.mp4))" || bad "trim $(info $W/trim.mp4) vs $ref $(info $W/$ref.mp4)"
  n1=$(info $W/trim.mp4 | cut -d, -f3); n2=$(info $W/$ref.mp4 | cut -d, -f3)
  python3 -I -c "import sys; sys.exit(abs($n1 - $n2) > 1)" && ok "trim frames $n1 ($ref $n2)" || bad "trim frames $n1 vs $ref $n2"
  s=$(ffmpeg -i $W/trim.mp4 -i $W/$ref.mp4 -lavfi "[0:v][1:v]ssim" -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
  python3 -I -c "import sys; sys.exit(${s:-0} < 0.95)" && ok "trim SSIM $s against $ref" || bad "trim SSIM ${s:-none} against $ref"
  if ffprobe -v error -select_streams a -show_entries stream=codec_type -of csv=p=0 $in | grep -q audio; then
    l1=$(ffmpeg -nostats -i $W/trim.mp4 -af ebur128 -f null - 2>&1 | grep -E '^\s+I:' | tail -1 | grep -oE '[-0-9.]+'); l2=$(ffmpeg -nostats -i $W/$ref.mp4 -af ebur128 -f null - 2>&1 | grep -E '^\s+I:' | tail -1 | grep -oE '[-0-9.]+')
    python3 -I -c "import sys; sys.exit(abs($l1 - ($l2)) > 0.5)" && ok "trim's sound $l1 LUFS ($ref $l2)" || bad "trim's sound $l1 LUFS vs $ref $l2"
  fi
done
# the remux: the same packets as ffmpeg's and rdb's -c copy (frames, length; the pictures equal)
ffmpeg -v error -y -i $in -c copy $W/ff-remux.mp4; $R ffmpeg -v error -y -i $in -c copy $W/rdb-remux.mp4
for ref in ff-remux rdb-remux; do
  n1=$(info $W/remux.mp4 | cut -d, -f3); n2=$(info $W/$ref.mp4 | cut -d, -f3)
  python3 -I -c "import sys; sys.exit(abs($n1 - $n2) > 1)" && ok "remux frames $n1 ($ref $n2)" || bad "remux frames $n1 vs $ref $n2"
  s=$(ffmpeg -i $W/remux.mp4 -i $W/$ref.mp4 -lavfi "[0:v][1:v]ssim" -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
  python3 -I -c "import sys; sys.exit(${s:-0} < 0.95)" && ok "remux SSIM $s against $ref" || bad "remux SSIM ${s:-none} against $ref"
done
[ "$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 $W/remux.mp4)" = "$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 $in)" ] && ok "remux kept the codec ($(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 $in)), nothing re-encoded" || bad "remux changed the codec"
# the proxy: size and frames against ffmpeg's; the pictures
ffmpeg -v error -y -i $in -vf scale=-2:720 -an $W/ff-proxy.mp4
[ "$(info $W/proxy.mp4)" = "$(info $W/ff-proxy.mp4)" ] && ok "proxy $(info $W/proxy.mp4) (ffmpeg $(info $W/ff-proxy.mp4))" || { n1=$(info $W/proxy.mp4); n2=$(info $W/ff-proxy.mp4); [ "${n1%,*}" = "${n2%,*}" ] && ok "proxy size ${n1%,*}, frames ${n1##*,} (ffmpeg ${n2##*,})" || bad "proxy $n1 vs ffmpeg $n2"; }
s=$(ffmpeg -i $W/proxy.mp4 -i $W/ff-proxy.mp4 -lavfi ssim -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
python3 -I -c "import sys; sys.exit(${s:-0} < 0.93)" && ok "proxy SSIM $s against ffmpeg" || bad "proxy SSIM ${s:-none} against ffmpeg"
# the heavy jobs (transcode: one native call): the proxy against ffmpeg's; the trim, with its sound, against ffmpeg's
[ "$(info $W/heavy-proxy.mp4 | cut -d, -f1,2)" = "$(info $W/ff-proxy.mp4 | cut -d, -f1,2)" ] && ok "native proxy $(info $W/heavy-proxy.mp4) (ffmpeg $(info $W/ff-proxy.mp4))" || bad "native proxy $(info $W/heavy-proxy.mp4) vs ffmpeg $(info $W/ff-proxy.mp4)"
s=$(ffmpeg -i $W/heavy-proxy.mp4 -i $W/ff-proxy.mp4 -lavfi "[0:v]fps=30[a];[1:v]fps=30[b];[a][b]ssim" -f null - 2>&1 | grep -o 'All:[0-9.]*' | cut -d: -f2)
python3 -I -c "import sys; sys.exit(${s:-0} < 0.93)" && ok "native proxy SSIM $s against ffmpeg" || bad "native proxy SSIM ${s:-none} against ffmpeg"
ffmpeg -v error -y -ss 1 -t 2 -i $in $W/ff-trim-av.mp4
if ffprobe -v error -select_streams a -show_entries stream=codec_type -of csv=p=0 $in | grep -q audio; then
  l1=$(ffmpeg -nostats -i $W/heavy-trim.mp4 -af ebur128 -f null - 2>&1 | grep -E '^\s+I:' | tail -1 | grep -oE '[-0-9.]+'); l2=$(ffmpeg -nostats -i $W/ff-trim-av.mp4 -af ebur128 -f null - 2>&1 | grep -E '^\s+I:' | tail -1 | grep -oE '[-0-9.]+')
  python3 -I -c "import sys; sys.exit(abs($l1 - ($l2)) > 0.5)" && ok "native trim's sound $l1 LUFS (ffmpeg $l2)" || bad "native trim's sound $l1 LUFS vs ffmpeg $l2"
fi
n1=$(info $W/heavy-trim.mp4 | cut -d, -f3); n2=$(info $W/ff-trim-av.mp4 | cut -d, -f3)
python3 -I -c "import sys; sys.exit(abs($n1 - $n2) > 1)" && ok "native trim frames $n1 (ffmpeg $n2)" || bad "native trim frames $n1 vs ffmpeg $n2"
# the sound, decoded: 16 kHz mono and at its own rate, mediabunny and rdb each against ffmpeg (plain decodes, packets
# end to end: a phone's audio timestamp gap closes up in all three)
cmp16() {
  python3 -I - $1 $2 $3 <<'PY'
import sys, wave, struct, math
def read(p):
    # 16-bit PCM WAV, plain or extensible (ffmpeg writes WAVE_FORMAT_EXTENSIBLE past 48 kHz): the fmt and data chunks
    b = open(p, 'rb').read(); i = 12; rate = ch = 0; data = b''
    while i + 8 <= len(b):
        cid, n = b[i:i + 4], struct.unpack('<I', b[i + 4:i + 8])[0]
        if cid == b'fmt ':
            ch, rate = struct.unpack('<HI', b[i + 10:i + 16])
        elif cid == b'data':
            data = b[i + 8:i + 8 + n]
        i += 8 + n + (n & 1)
    return [x / 32768 for x in struct.unpack('<%dh' % (len(data) // 2), data[:len(data) // 2 * 2])], rate, ch
a, ra, ca = read(sys.argv[1]); b, rb, cb = read(sys.argv[2])
db = lambda v: 10 * math.log10(sum(x * x for x in v[::5]) / max(1, len(v[::5])) + 1e-20)
n = min(len(a), len(b)); s = sum(a[i] * b[i] for i in range(0, n, 3)); sa = sum(a[i] ** 2 for i in range(0, n, 3)); sb = sum(b[i] ** 2 for i in range(0, n, 3))
corr = s / math.sqrt(sa * sb + 1e-20)
print(f"      {sys.argv[3]}: {len(a)} vs {len(b)} samples at {ra}/{rb} Hz, {ca}/{cb} ch, level {db(a):.2f} vs {db(b):.2f} dB, correlation {corr:.4f}")
sys.exit(not (ra == rb and ca == cb and abs(len(a) - len(b)) <= 0.05 * ra and abs(db(a) - db(b)) < 0.3 and corr > 0.98))
PY
}
if [ -f $W/a16.wav ]; then
  ffmpeg -v error -y -i $in -ac 1 -ar 16000 $W/ff.wav; $R ffmpeg -v error -y -i $in -ac 1 -ar 16000 $W/rdb.wav
  ffmpeg -v error -y -i $in -vn $W/ff-native.wav; $R ffmpeg -v error -y -i $in $W/rdb-native.wav
  gaps=$(ffprobe -v error -select_streams a -show_entries packet=pts_time,duration_time -of csv=p=0 $in | awk -F, 'NR>1 && $1-p > 1.5*d {n++} {p=$1; d=$2} END {print n+0}')
  [ "$gaps" -gt 0 ] && echo "      the file's audio has $gaps timestamp gap(s); all three decode its packets end to end"
  cmp16 $W/a16.wav $W/ff.wav "16 kHz mediabunny vs ffmpeg" && ok "16 kHz audio: mediabunny = ffmpeg" || bad "16 kHz audio: mediabunny vs ffmpeg"
  cmp16 $W/rdb.wav $W/ff.wav "16 kHz rdb vs ffmpeg" && ok "16 kHz audio: rdb ffmpeg = ffmpeg" || bad "16 kHz audio: rdb ffmpeg vs ffmpeg"
  cmp16 $W/anative.wav $W/ff-native.wav "own rate mediabunny vs ffmpeg" && ok "audio at its own rate: mediabunny = ffmpeg" || bad "audio at its own rate: mediabunny vs ffmpeg"
  cmp16 $W/rdb-native.wav $W/ff-native.wav "own rate rdb vs ffmpeg" && ok "audio at its own rate: rdb ffmpeg = ffmpeg" || bad "audio at its own rate: rdb ffmpeg vs ffmpeg"
fi
rm -rf $W
echo "mediabunny vs ffmpeg: $fails failing"
exit $((fails > 0))
