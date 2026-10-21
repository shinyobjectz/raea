#!/usr/bin/env python3
"""Golden video with sound (README point 4): a render against an approved reference mp4, picture and audio.

Picture: SSIM at the case's frames (every cut, every designed part's entrance, the last frame). Sound: the same
length, integrated loudness within lufs_tol LU, the loudness envelope (20 ms RMS) correlated at env_min or better,
and the voice in sync: the lag that best lines the envelopes up within sync_max frames. Real ffmpeg measures both
files, so the grader doesn't depend on what it grades.

Usage: golden_av.py CASE_DIR RENDER.mp4        (CASE_DIR holds case.json and reference/approved.mp4)
Exit 0 when every check passes."""
import json, math, os, re, struct, subprocess, sys, tempfile


def run(*a):
    return subprocess.run(a, capture_output=True, text=True)


def frame_png(mp4, n, out):
    run("ffmpeg", "-v", "error", "-y", "-i", mp4, "-vf", f"select=eq(n\\,{n})", "-vsync", "0", "-frames:v", "1", out)
    return os.path.exists(out)


def ssim(a, b):
    m = re.search(r"All:([0-9.]+)", run("ffmpeg", "-hide_banner", "-i", a, "-i", b, "-lavfi", "ssim", "-f", "null", "-").stderr)
    return float(m.group(1)) if m else 0.0


def lufs(mp4):
    m = re.findall(r"I:\s+(-?[0-9.]+) LUFS", run("ffmpeg", "-hide_banner", "-i", mp4, "-af", "ebur128", "-f", "null", "-").stderr)
    return float(m[-1]) if m else float("nan")


def duration(mp4):
    return float(run("ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", mp4).stdout.strip() or 0)


def envelope(mp4, rate=16000, win=0.02):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", mp4, "-f", "f32le", "-ac", "1", "-ar", str(rate), "-"], capture_output=True).stdout
    xs = struct.unpack(f"<{len(raw) // 4}f", raw[: len(raw) // 4 * 4])
    n = int(rate * win)
    return [math.sqrt(sum(v * v for v in xs[i:i + n]) / n) for i in range(0, len(xs) - n + 1, n)]


def corr(a, b):
    k = min(len(a), len(b))
    a, b = a[:k], b[:k]
    ma, mb = sum(a) / k, sum(b) / k
    num = sum((x - ma) * (y - mb) for x, y in zip(a, b))
    den = math.sqrt(sum((x - ma) ** 2 for x in a) * sum((y - mb) ** 2 for y in b))
    return num / den if den else 0.0


def best_lag(a, b, max_lag):
    best = (0, -2.0)
    for lag in range(-max_lag, max_lag + 1):
        x, y = (a[lag:], b) if lag >= 0 else (a, b[-lag:])
        c = corr(x, y)
        if c > best[1]:
            best = (lag, c)
    return best


def main():
    case, render = sys.argv[1], sys.argv[2]
    cfg = json.load(open(os.path.join(case, "case.json")))
    ref = os.path.join(case, "reference", "approved.mp4")
    fps = cfg.get("fps", 30)
    ok = True
    def say(passed, what):
        nonlocal ok
        ok &= passed
        print(("PASS  " if passed else "FAIL  ") + what)
    dr, dg = duration(ref), duration(render)
    say(abs(dr - dg) <= 1.5 / fps, f"length {dg:.3f} s against {dr:.3f} s")
    with tempfile.TemporaryDirectory() as t:
        for n in cfg["frames"]:
            a, b = f"{t}/r{n}.png", f"{t}/g{n}.png"
            if not (frame_png(ref, n, a) and frame_png(render, n, b)):
                say(False, f"frame {n}: missing in one of the files"); continue
            s = ssim(b, a)
            say(s >= cfg.get("ssim_min", 0.95), f"frame {n} ssim {s:.4f}")
    lr, lg = lufs(ref), lufs(render)
    say(abs(lr - lg) <= cfg.get("lufs_tol", 0.5), f"loudness {lg:.1f} LUFS against {lr:.1f}")
    er, eg = envelope(ref), envelope(render)
    if not eg or not er:
        say(False, "sound: " + ("the render has none" if not eg else "the reference has none"))
        sys.exit(1)
    max_lag = 25  # look 500 ms either way, so a drift is measured, not just flagged
    lag, c = best_lag(eg, er, max_lag)
    say(c >= cfg.get("env_min", 0.95), f"loudness envelope correlation {c:.4f}")
    say(abs(lag * 0.02) <= cfg.get("sync_max_frames", 1) / fps + 1e-9, f"sync {lag * 20:+d} ms")
    sys.exit(0 if ok else 1)


main()
