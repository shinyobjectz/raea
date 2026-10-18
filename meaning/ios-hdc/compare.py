"""Track L, part 7's proof: the clip the probe app encodes on the iPhone (app/inputs/clip.mp4), encoded on the Mac the
same way (ffmpeg frames at 8 fps squashed to 256, the fused SigLIP2SeenBits model, YAMNet's Core ML package in 10 s
chunks every 9.6 s, then `raea-meaning encode`), and the phone's records (fetched into phone-enc/) compared slot by slot:
the share of bits that agree. Frame decode and scaling differ between AVFoundation and ffmpeg, and Core ML runs on
different silicon, so some drift is expected; the slots whose fillers are exact functions of the same inputs should
agree completely.

    ~/.cache/raea-sao/.venv/bin/python compare.py mac        # the Mac's records -> mac-enc/
    ~/.cache/raea-sao/.venv/bin/python compare.py compare    # after copying the phone's Documents/enc to phone-enc/
"""
import glob, json, os, subprocess, sys
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
CLIP = f"{HERE}/app/inputs/clip.mp4"
MODELS = f"{HERE}/app/models"
MEAN = os.path.expanduser("~/raea/meaning/target/release/raea-meaning")


def mac():
    import coremltools as ct
    from PIL import Image
    out = f"{HERE}/mac-enc"; os.makedirs(f"{out}/fr", exist_ok=True)
    for f in glob.glob(f"{out}/fr/*.png"):
        os.remove(f)
    subprocess.run(["ffmpeg", "-v", "error", "-i", CLIP, "-vf", "fps=8,scale=256:256:flags=bilinear", f"{out}/fr/%05d.png"], check=True)
    sig = ct.models.MLModel(f"{MODELS}/SigLIP2SeenBits.mlpackage", compute_units=ct.ComputeUnit.CPU_AND_NE)
    vec = []
    for f in sorted(glob.glob(f"{out}/fr/*.png")):
        v = sig.predict({"image": Image.open(f).convert("RGB")})["vector"].reshape(-1)
        vec.append(v / max(np.linalg.norm(v), 1e-12))
    V = np.array(vec, np.float32); V.tofile(f"{out}/sig.f32")
    json.dump({"source": CLIP, "model": "siglip2-base-p16-256-coreml-palettized8", "fps": 8.0, "n": len(V), "dim": 768}, open(f"{out}/sig.json", "w"))
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", CLIP, "-ac", "1", "-ar", "16000", "-f", "f32le", "-"], capture_output=True, check=True).stdout
    pcm = np.frombuffer(raw, np.float32)
    rows = (len(pcm) - 15600) // 7680 + 1 if len(pcm) >= 15600 else (1 if len(pcm) else 0)
    ym = ct.models.MLModel(f"{MODELS}/YAMNet.mlpackage", compute_units=ct.ComputeUnit.CPU_AND_NE)
    emb, off = [], 0
    while len(emb) < rows:
        w = np.zeros(160000, np.float32); seg = pcm[off:off + 160000]; w[:len(seg)] = seg
        e = ym.predict({"waveform": w})["embeddings"]
        emb += [r for r in e][: rows - len(emb)]; off += 153600
    np.array(emb, np.float32).tofile(f"{out}/yam.emb.f32")
    json.dump({"n": len(emb)}, open(f"{out}/yam.json", "w"))
    subprocess.run([MEAN, "encode", f"{out}/sig", "--yamnet", f"{out}/yam", "-o", f"{out}/rec", "--fillers"], check=True)
    print(f"mac: {len(V)} moments, {len(emb)} YAMNet rows -> {out}")


def compare():
    ph, mc = f"{HERE}/phone-enc", f"{HERE}/mac-enc"
    res = {}
    for f in sorted(glob.glob(f"{ph}/*.hv")):
        name = os.path.basename(f)
        g = f"{mc}/rec.{name}" if name.startswith(("m.", "w.")) else f"{mc}/rec.{name}"
        if not os.path.exists(g):
            continue
        a, b = np.fromfile(f, "<u8"), np.fromfile(g, "<u8")
        n = min(len(a), len(b)) // 128 * 128
        x = np.unpackbits(np.bitwise_xor(a[:n], b[:n]).view(np.uint8))
        rows = n // 128
        res[name] = {"rows_phone": len(a) // 128, "rows_mac": len(b) // 128, "bit_agreement": round(1 - float(x.mean()), 5),
                     "identical_rows": int(sum(np.array_equal(a[i * 128:(i + 1) * 128], b[i * 128:(i + 1) * 128]) for i in range(rows)))}
        print(f"{name:<18} rows phone {res[name]['rows_phone']:4d} mac {res[name]['rows_mac']:4d}  bits agree {res[name]['bit_agreement']:.5f}  identical rows {res[name]['identical_rows']}")
    json.dump(res, open(f"{HERE}/compare.json", "w"), indent=1)


if __name__ == "__main__":
    {"mac": mac, "compare": compare}[sys.argv[1]]()
