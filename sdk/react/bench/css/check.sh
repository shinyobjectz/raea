#!/bin/sh
# A composition's CSS imports and CSS modules reach its page (docs/vite-in-raea.md A2): edit.tsx bundled in process
# (RAEA_BUNDLER=rolldown), its first frame rendered, five pixels checked against what Chrome paints for the same CSS.
#   sdk/react/bench/css/check.sh [RAEA_MOTION]
# Build first: lane build -- cargo build --release --bin raea-motion (in react/).
set -e
cd "$(dirname "$0")/../.."
BIN=${1:-target/release/raea-motion}
OUT=${TMPDIR:-/tmp}/raea-css-check
mkdir -p "$OUT"
RAEA_BUNDLER=rolldown RAEA_BUNDLE_CACHE=0 RAEA_AUDIO_MUTE=1 RAEA_READBACK=rgba "$BIN" render bench/css/edit.tsx -o "$OUT/frame.png" --frame 0 > "$OUT/render.log" 2>&1 || { echo "css: FAIL the render failed (see $OUT/render.log)"; exit 1; }
python3 - "$OUT/frame.png" <<'PY'
import struct, sys, zlib
def png(path):
    data = open(path, "rb").read()
    pos, chunks = 8, []
    while pos < len(data):
        n, t = struct.unpack(">I4s", data[pos:pos + 8]); chunks.append((t, data[pos + 8:pos + 8 + n])); pos += 12 + n
    w, h, depth, ctype = struct.unpack(">IIBB", [c for t, c in chunks if t == b"IHDR"][0][:10])
    raw = zlib.decompress(b"".join(c for t, c in chunks if t == b"IDAT"))
    bpp = {2: 3, 6: 4}[ctype]; stride = w * bpp; rows, prev = [], bytearray(stride); i = 0
    for _ in range(h):
        f = raw[i]; line = bytearray(raw[i + 1:i + 1 + stride]); i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0; b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            line[x] = (line[x] + [0, a, b, (a + b) // 2, a if pa <= pb and pa <= pc else (b if pb <= pc else c)][f]) & 255
        rows.append(line); prev = line
    return lambda x, y: tuple(rows[y][x * bpp:x * bpp + 3])
px = png(sys.argv[1])
# (x, y): Chrome's color for the same page, and what paints it
want = [((20, 20), (255, 0, 0), "site.css .left"), ((100, 100), (255, 255, 0), "nested .left .dot"),
        ((300, 100), (0, 0, 255), "card.module.css .right (composes .box, color-mix)"), ((390, 190), (0, 0, 255), "the module's size")]
bad = [f"{what} at {xy}: {px(*xy)} not {c}" for xy, c, what in want if max(abs(a - b) for a, b in zip(px(*xy), c)) > 2]
print("css: " + ("PASS  " + ", ".join(w for _, _, w in want) if not bad else "FAIL  " + "; ".join(bad)))
sys.exit(1 if bad else 0)
PY

# CSS animations run on the composition's time, from 0 or a <Sequence>'s start, the same for a frame rendered alone
# and in a render (anim.tsx; dom.rs set_time, set_animation_clips)
"$BIN" render bench/css/anim.tsx -o "$OUT/anim.mp4" > "$OUT/anim.log" 2>&1 || { echo "css animations: FAIL the render failed (see $OUT/anim.log)"; exit 1; }
for f in 15 25; do
  RAEA_READBACK=rgba "$BIN" render bench/css/anim.tsx -o "$OUT/anim-$f.png" --frame "$f" >> "$OUT/anim.log" 2>&1
  ffmpeg -v error -y -i "$OUT/anim.mp4" -vf "select=eq(n\,$f)" -vframes 1 "$OUT/anim-mp4-$f.png"
done
python3 - "$OUT" <<'PY'
import struct, sys, zlib
def png(path):
    data = open(path, "rb").read(); pos, chunks = 8, []
    while pos < len(data):
        n, t = struct.unpack(">I4s", data[pos:pos + 8]); chunks.append((t, data[pos + 8:pos + 8 + n])); pos += 12 + n
    w, h, depth, ctype = struct.unpack(">IIBB", [c for t, c in chunks if t == b"IHDR"][0][:10])
    raw = zlib.decompress(b"".join(c for t, c in chunks if t == b"IDAT")); bpp = {2: 3, 6: 4}[ctype]; stride = w * bpp; rows, prev = [], bytearray(stride); i = 0
    for _ in range(h):
        f = raw[i]; line = bytearray(raw[i + 1:i + 1 + stride]); i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0; b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            line[x] = (line[x] + [0, a, b, (a + b) // 2, a if pa <= pb and pa <= pc else (b if pb <= pc else c)][f]) & 255
        rows.append(line); prev = line
    def left(y, col):
        xs = [x for x in range(w) if all(abs(rows[y][x * bpp + k] - col[k]) < 40 for k in range(3))]
        return xs[0] if xs else None
    return left
out, bad = sys.argv[1], []
# (frame, box, y, colour, x): 600 px in 1 s at 30 fps; B starts at frame 10; C (in the render) at frame 1
want = [(15, "A", 25, (255, 0, 0), 300), (25, "A", 25, (255, 0, 0), 500), (15, "B", 125, (0, 0, 255), 100), (25, "B", 125, (0, 0, 255), 300)]
for f, box, y, col, x in want:
    for kind in ("alone", "mp4"):
        got = png(f"{out}/anim-{f}.png" if kind == "alone" else f"{out}/anim-mp4-{f}.png")(y, col)
        if got is None or abs(got - x) > 3:
            bad.append(f"{box} at frame {f} ({kind}): x {got}, not {x}")
c = png(f"{out}/anim-mp4-15.png")(225, (0, 160, 0))
if c is None or abs(c - 280) > 3:
    bad.append(f"C's transition at frame 15: x {c}, not 280")
print("css animations: " + ("PASS  keyframes from 0 and from a Sequence's start, alone and in a render; a transition" if not bad else "FAIL  " + "; ".join(bad)))
sys.exit(1 if bad else 0)
PY

# React's camelCase SVG props are SVG's attributes (svg.tsx; lib.rs attr_name)
RAEA_READBACK=rgba "$BIN" render bench/css/svg.tsx -o "$OUT/svg.png" --frame 0 > "$OUT/svg.log" 2>&1 || { echo "svg props: FAIL the render failed (see $OUT/svg.log)"; exit 1; }
python3 - "$OUT/svg.png" <<'PY'
import struct, sys, zlib
def png(path):
    data = open(path, "rb").read(); pos, chunks = 8, []
    while pos < len(data):
        n, t = struct.unpack(">I4s", data[pos:pos + 8]); chunks.append((t, data[pos + 8:pos + 8 + n])); pos += 12 + n
    w, h, depth, ctype = struct.unpack(">IIBB", [c for t, c in chunks if t == b"IHDR"][0][:10])
    raw = zlib.decompress(b"".join(c for t, c in chunks if t == b"IDAT")); bpp = {2: 3, 6: 4}[ctype]; stride = w * bpp; rows, prev = [], bytearray(stride); i = 0
    for _ in range(h):
        f = raw[i]; line = bytearray(raw[i + 1:i + 1 + stride]); i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0; b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            line[x] = (line[x] + [0, a, b, (a + b) // 2, a if pa <= pb and pa <= pc else (b if pb <= pc else c)][f]) & 255
        rows.append(line); prev = line
    return lambda x, y: tuple(rows[y][x * bpp:x * bpp + 3])
px = png(sys.argv[1])
# (x, y): the colour Chrome paints there, and what paints it
want = [((150, 25), (255, 0, 0), "strokeWidth (20 px above the line's centre)"), ((60, 140), (128, 128, 255), "fillOpacity 0.5 over white"),
        ((175, 140), (0, 0, 0), "a dash"), ((225, 140), (255, 255, 255), "strokeDasharray's gap")]
bad = [f"{what} at {xy}: {px(*xy)} not {c}" for xy, c, what in want if max(abs(a - b) for a, b in zip(px(*xy), c)) > 3]
print("svg props: " + ("PASS  " + ", ".join(w for _, _, w in want) if not bad else "FAIL  " + "; ".join(bad)))
sys.exit(1 if bad else 0)
PY

# A video (a texture the GPU holds, as the 3D is) inside a rotated box that clips: drawn, at the box's rotation and cut
# by its rounded clip (quads/record.rs clip_held; it was dropped, leaving the box's background)
mkdir -p "$OUT/rot"
ffmpeg -v error -y -f lavfi -i color=c=red:s=320x192:d=1:r=30 -pix_fmt yuv420p -c:v libx264 "$OUT/rot/red.mp4"
cat > "$OUT/rot/edit.tsx" <<TSX
import { AbsoluteFill, Composition, Video, registerRoot } from "@raea/motion";
function C() {
  return (
    <AbsoluteFill style={{ background: "#fff" }}>
      <div style={{ position: "absolute", left: 60, top: 60, width: 200, height: 120, overflow: "hidden", borderRadius: 24, transform: "rotate(-12deg)", background: "rgb(0,200,0)" }}>
        <Video src="$OUT/rot/red.mp4" style={{ width: 300, height: 180, marginLeft: -50, marginTop: -30 }} />
      </div>
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="rot" component={C} width={320} height={240} fps={30} durationInFrames={10} />);
TSX
RAEA_AUDIO_MUTE=1 RAEA_READBACK=rgba "$BIN" render "$OUT/rot/edit.tsx" -o "$OUT/rot.png" --frame 3 > "$OUT/rot.log" 2>&1 || { echo "rotated clip: FAIL the render failed (see $OUT/rot.log)"; exit 1; }
python3 - "$OUT/rot.png" <<'PY'
import struct, sys, zlib
def png(path):
    data = open(path, "rb").read(); pos, chunks = 8, []
    while pos < len(data):
        n, t = struct.unpack(">I4s", data[pos:pos + 8]); chunks.append((t, data[pos + 8:pos + 8 + n])); pos += 12 + n
    w, h, depth, ctype = struct.unpack(">IIBB", [c for t, c in chunks if t == b"IHDR"][0][:10])
    raw = zlib.decompress(b"".join(c for t, c in chunks if t == b"IDAT")); bpp = {2: 3, 6: 4}[ctype]; stride = w * bpp; rows, prev = [], bytearray(stride); i = 0
    for _ in range(h):
        f = raw[i]; line = bytearray(raw[i + 1:i + 1 + stride]); i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0; b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            line[x] = (line[x] + [0, a, b, (a + b) // 2, a if pa <= pb and pa <= pc else (b if pb <= pc else c)][f]) & 255
        rows.append(line); prev = line
    return lambda x, y: tuple(rows[y][x * bpp:x * bpp + 3])
px = png(sys.argv[1])
red = lambda c: c[0] > 200 and c[1] < 60 and c[2] < 60
white = lambda c: min(c) > 240
# the box's centre is (160, 120); points in its frame turned -12 degrees: 15 px above its top edge (the video reaches
# there, the clip doesn't) and past its top-right rounded corner
want = [((160, 120), red, "the video, not the box's background"), ((144, 47), white, "cut by the rotated clip"), ((244, 43), white, "cut by its rounded corner")]
bad = [f"{what} at {xy}: {px(*xy)}" for xy, ok, what in want if not ok(px(*xy))]
print("rotated clip: " + ("PASS  " + ", ".join(w for _, _, w in want) if not bad else "FAIL  " + "; ".join(bad)))
sys.exit(1 if bad else 0)
PY
