#!/bin/bash
# G8 step 0 on the current UGC edit, through the step 0 path (RGBA readback): reference hashes, the stage baseline,
# the four encodes, playback, and the decode rows for the rotated b-roll. Every render is guarded.
set -u
cd "$(dirname "$0")/../examples/ugc"
B=../../target/release/rdb
G=../../bench/guarded.sh
R=../out/ref
M=~/raea/media
mkdir -p $R
export RAEA_READBACK=rgba
SH=$(python3 -c "import random;l=list(range(738));random.Random(7).shuffle(l);print(','.join(map(str,l)))")
echo "== decode rows, broll-rot"
[ -f $M/fixtures/broll-rot.av1.mp4 ] || ffmpeg -v error -y -i $M/fixtures/broll-rot.mov -an -c:v libsvtav1 -crf 28 -g 30 -preset 8 -pix_fmt yuv420p $M/fixtures/broll-rot.av1.mp4 2>/dev/null
for spec in "ffmpeg broll-rot.mov" "apple broll-rot.mov" "soft broll-rot.mov" "av1 broll-rot.av1.mp4"; do
  set -- $spec
  s=$($M/target/release/raea-media seek $1 $M/fixtures/$2 --ref $M/fixtures/$2 2>/dev/null | tail -1)
  d=$($M/target/release/raea-media decode $1 $M/fixtures/$2 --n 300 2>/dev/null | tail -1)
  echo "$1 $2 $s $d"
done
echo "== reference hashes"
for m in apple soft; do
  for run in a b shuffled; do
    at=""; [ $run = shuffled ] && at="--at $SH"
    RAEA_MEDIA=$m $G $B render ugc.sql -o $R/hash-$m-$run.txt $at 2>&1 | grep guarded
  done
  sort -n $R/hash-$m-shuffled.txt > $R/s && mv $R/s $R/hash-$m-shuffled.txt
  echo "$m: a==b $(cmp -s $R/hash-$m-a.txt $R/hash-$m-b.txt && echo yes || echo NO), a==shuffled $(cmp -s $R/hash-$m-a.txt $R/hash-$m-shuffled.txt && echo yes || echo NO), frames $(wc -l < $R/hash-$m-a.txt | tr -d ' ')"
done
echo "== baseline stages"
for mode in pipelined serial; do
  if [ $mode = serial ]; then export RAEA_SERIAL=1; else unset RAEA_SERIAL; fi
  RAEA_MEDIA=apple $G $B render ugc.sql -o ../out/base-$mode.mp4 --codec apple --kbps 8000 2>&1 | grep -E "^\{" > ../../bench/baseline-$mode.json
done
unset RAEA_SERIAL
echo "== encodes"
rm -f ../out/encode-table.jsonl
for combo in "apple apple" "soft soft" "ffmpeg ffmpeg" "soft av1"; do
  set -- $combo
  RAEA_MEDIA=$1 $G $B render ugc.sql -o ../out/ugc-$2.mp4 --codec $2 --kbps 8000 --ssim > ../out/enc-$2.log 2>&1
  grep -E "^\{" ../out/enc-$2.log | sed "s/^/{\"decode\":\"$1\",\"enc\":\"$2\",\"r\":/; s/$/}/" >> ../out/encode-table.jsonl
  grep "guarded:" ../out/enc-$2.log
done
cp ../out/encode-table.jsonl ../../bench/encode-table.jsonl
echo "== playback"
$M/target/release/raea-media playable ../out/ugc-{apple,soft,ffmpeg,av1}.mp4 2>/dev/null > ../../bench/playback.jsonl
for f in apple soft ffmpeg av1; do echo "$f $(ffprobe -v error -count_frames -show_entries stream=codec_name,nb_read_frames -of csv=p=0 ../out/ugc-$f.mp4 | tr '\n' ' ')"; done
echo "== done"
