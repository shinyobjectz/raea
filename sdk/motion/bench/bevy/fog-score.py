#!/usr/bin/env python3
"""Volumetric fog scored against its ground truth, so "better fog" is a number: each scene's stills against the same
frames drawn by the reference (RAEA_FOG_REFERENCE=1: Bevy's march at 512 steps, undithered, at full resolution, as
scene3d::fog_reference sets it; 1024 drops the fog on a 1080p frame), SSIM and PSNR by frame; and, over a clip of each scene, the candidate's cost in ms a
frame (RAEA_FRAME_LOG, the median of the clip's frames) and its bitrate encoded as the share copies are (x265 CRF 24).

  sdk/motion/bench/bevy/fog-score.py [--label L] [--env K=V ...] [--only a,b] [--ref bevy|grid] [--no-clip] [--fresh-ref] [--fresh-src]

--ref grid scores against the grid's own ground truth instead (RAEA_FOG_REFERENCE=grid: the grid's medium and lights,
marched per pixel at 1024 steps with no grid and no hero sampling, froxel_light.wgsl's `reference`), which draws what
Bevy's march can't: a volume's falloff and the light of a <Fire>.

The candidate is the composition as it asks (its own steps, dither, resolution) with --env's variables set, e.g.
`--env RAEA_FOG_MODE=march --label march` (Bevy's march, which a composition asks for with mode="march"; the
froxel grid is the default) or `--env RAEA_FOG_HERO=0 --label grid`. Each scene's sources are cloned (APFS, copy on write) into
OUT/src/<scene> the first time and drawn from there after, so other sessions' edits don't move the ground under a
comparison; --fresh-src clones them again (and --fresh-ref draws the reference again). Scenes needing media that git
doesn't hold (Rome's) are cloned from RAEA_FOG_SCENE_ROOT (default ~/raea), the fixtures from this tree. OUT is $TMPDIR/raea-fog-score; each run writes
OUT/score-<label>.json and prints a table.
"""
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SCENE_ROOT = Path(os.environ.get("RAEA_FOG_SCENE_ROOT", Path.home() / "raea")).expanduser()
NOIR = Path.home() / "noir-doc"
BIN = os.environ.get("RAEA_MOTION_BIN", str(ROOT / "sdk/react/target/release/raea-motion"))
OUT = Path(os.environ.get("TMPDIR", "/tmp")) / "raea-fog-score"

# name: (base the folders are cloned from, folders, the edit inside the clone, stills, clip [first, last] or None)
SCENES = {
    "volumetric": (ROOT, ["sdk/motion/bench/bevy/volumetric"], "sdk/motion/bench/bevy/volumetric/edit.tsx", [15, 45], (0, 59)),
    "volumetric-dither": (ROOT, ["sdk/motion/bench/bevy/volumetric-dither"], "sdk/motion/bench/bevy/volumetric-dither/edit.tsx", [15, 45], (0, 59)),
    "volumetric-half": (ROOT, ["sdk/motion/bench/bevy/volumetric-half"], "sdk/motion/bench/bevy/volumetric-half/edit.tsx", [15, 45], (0, 59)),
    "volumetric-froxel": (ROOT, ["sdk/motion/bench/bevy/volumetric-froxel"], "sdk/motion/bench/bevy/volumetric-froxel/edit.tsx", [15, 45], (0, 59)),
    "volumetric-grid": (ROOT, ["sdk/motion/bench/bevy/volumetric-grid"], "sdk/motion/bench/bevy/volumetric-grid/edit.tsx", [15, 45], (0, 59)),
    "rome-plot": (SCENE_ROOT, ["apps/void/brand/rome"], "apps/void/brand/rome/film.tsx", [1560, 1900], (1560, 1607)),
    "rome-curia": (SCENE_ROOT, ["apps/void/brand/rome"], "apps/void/brand/rome/film.tsx", [4185, 4235], (4185, 4232)),
    "noir-mean": (NOIR, ["edit/plates", "edit/media", "assets/3d"], "edit/plates/ink-mean.tsx", [120, 400], (120, 167)),
}

# what's taken out of a scene's frozen edit before it's drawn: the noir film's ink <Look> thresholds and dithers the
# whole frame to two tones, so any fog difference flips pixels and its PSNR says nothing about the fog; scored on the
# 3D under it (and its outline) instead
STRIP = {"noir-mean": [r"\s*<Look mode=\"ink\"[^>]*/>", r"\s*<Outline [^>]*/>"]}


def run(cmd, env=None, log=None):
    # never from the bake cache: a hit replays a bake that has lost the scene's spot light (sdk/react/src/bake.rs, seen
    # 2026-10-20 on the volumetric fixture: the first render lit, every repeat without it)
    env = {"RAEA_BAKE_CACHE": "0", **(env or {})}
    print(f"fog-score: {Path(cmd[2]).name} {' '.join(map(str, cmd[3:5]))} {' '.join(f'{k}={v}' for k, v in env.items() if k != 'RAEA_BAKE_CACHE')}", flush=True)
    r = subprocess.run(cmd, env={**os.environ, **env}, capture_output=True, text=True)
    text = r.stdout + r.stderr
    if log:
        Path(log).write_text(text)
    said = next((l for l in text.splitlines() if l.startswith(("APPLIED", "REFUSED"))), "")
    if r.returncode or not said.startswith("APPLIED"):
        sys.exit(f"fog-score: {' '.join(map(str, cmd[:3]))} failed: {said or text[-400:]}")
    return said


def source(name, fresh):
    """The scene's edit in its frozen clone (sources shared by scenes with the same folders are cloned once)."""
    base, folders, edit, *_ = SCENES[name]
    key = re.sub(r"[^a-z0-9]+", "-", f"{base}-{'-'.join(folders)}".lower()).strip("-")[-80:]
    dst = OUT / "src" / key
    if fresh and dst.exists():
        shutil.rmtree(dst)
    if not dst.exists():
        for f in folders:
            (dst / f).parent.mkdir(parents=True, exist_ok=True)
            subprocess.run(["cp", "-cR", str(base / f), str(dst / f)], check=True)
    out = dst / edit
    if name in STRIP:
        # its own copy of the edit beside the shared one, the parts taken out
        text = out.read_text()
        for pat in STRIP[name]:
            text, n = re.subn(pat, "", text)
            if not n:
                sys.exit(f"fog-score: {name}: nothing matched {pat} in {edit}")
        out = out.with_name(f"{out.stem}.fog-score.tsx")
        out.write_text(text)
    return out


FLAKES = []


def stills(edit, frames, out_dir, env):
    """The frames drawn a process each (`--frame`), as out_dir/<frame>.png: `--at` draws the frames before the last
    too, and at the reference's steps that much work queued at once loses Metal's device. Each is drawn twice, and a third time
    when the two differ, the two that agree kept: under load a 3D frame sometimes draws without a spot light's light on
    its surfaces (its fog still lit; seen 2026-10-20 on volumetric-froxel, in bursts, the cause not yet found), and one
    such draw would move a score by more than any fog change."""
    out_dir.mkdir(parents=True, exist_ok=True)
    for f in frames:
        draws = []
        for k in range(3):
            png = out_dir / f"{f}.{k}.png"
            run([BIN, "render", str(edit), "--frame", str(f), "-o", str(png)], env, out_dir / f"render-{f}.log")
            draws.append(png)
            agree = [(a, b) for i, a in enumerate(draws) for b in draws[i + 1:] if compare(a, b)[1] >= 60.0]
            if agree:
                break
        if k > 1:
            FLAKES.append(f"{out_dir.parent.name}/{out_dir.name} {f}: {k + 1} draws{'' if agree else ', none agreeing (the last kept)'}")
        keep = agree[0][0] if agree else draws[-1]
        keep.replace(out_dir / f"{f}.png")
        for d in draws:
            d.unlink(missing_ok=True)


def compare(a, b):
    """SSIM and PSNR of two stills (RGB), by ffmpeg."""
    r = subprocess.run(["ffmpeg", "-v", "info", "-nostats", "-i", str(a), "-i", str(b), "-filter_complex", "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim=stats_file=-;[0]format=rgb24[c];[1]format=rgb24[d];[c][d]psnr", "-f", "null", "-"], capture_output=True, text=True)
    ssim = re.findall(r"All:([0-9.]+)", r.stdout + r.stderr)
    psnr = re.findall(r"average:([0-9.inf]+)", r.stdout + r.stderr)
    return float(ssim[-1]) if ssim else 0.0, (float(psnr[-1]) if psnr and psnr[-1] != "inf" else 99.0)


def clip_cost(edit, scenes, env, work):
    """One timed render of the whole composition (the export cache never answers a frame-logged one); for each scene
    on it, the median ms a frame over its clip and the clip's bitrate at x265 CRF 24."""
    work.mkdir(parents=True, exist_ok=True)
    mp4, log = work / "all.mp4", work / "frames.tsv"
    said = run([BIN, "render", str(edit), "-o", str(mp4)], {**env, "RAEA_FRAME_LOG": str(log)}, work / "render.log")
    wall = re.search(r"\(([0-9.]+) ms a frame", said)
    rows = {}
    for line in log.read_text().splitlines():
        cols = line.split("\t")
        try:
            rows[int(float(cols[0]))] = sum(float(c) for c in cols[1:])
        except (ValueError, IndexError):
            continue
    rate = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0", str(mp4)], capture_output=True, text=True).stdout.strip()
    num, _, den = rate.partition("/")
    fps = float(num) / float(den or 1)
    out = {}
    for name in scenes:
        a, b = SCENES[name][4]
        ms = [rows[f] for f in range(a, b + 1) if f in rows]
        enc = work / f"{name}.x265.mp4"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", f"{a / fps:.4f}", "-i", str(mp4), "-frames:v", str(b - a + 1), "-an", "-c:v", "libx265", "-crf", "24", "-preset", "medium", "-pix_fmt", "yuv420p", "-x265-params", "log-level=error", str(enc)], check=True)
        size = enc.stat().st_size
        out[name] = {"ms_frame": round(statistics.median(ms), 2) if ms else None, "wall_ms_frame_whole": float(wall.group(1)) if wall else None, "kbps_x265": round(size * 8 / ((b - a + 1) / fps) / 1000)}
    mp4.unlink()
    return out


def main():
    args = sys.argv[1:]
    label, env, only, reference = "authored", {}, list(SCENES), "bevy"
    i = 0
    while i < len(args):
        if args[i] == "--label":
            label, i = args[i + 1], i + 2
        elif args[i] == "--env":
            k, v = args[i + 1].split("=", 1)
            env[k], i = v, i + 2
        elif args[i] == "--ref":
            reference, i = args[i + 1], i + 2
        elif args[i] == "--only":
            only, i = args[i + 1].split(","), i + 2
        else:
            i += 1
    if not NOIR.exists():
        only = [n for n in only if n != "noir-mean"]
    fresh_src, fresh_ref, no_clip = "--fresh-src" in args, "--fresh-ref" in args, "--no-clip" in args
    result = {"label": label, "env": env, "reference": reference, "scenes": {}}
    ref_env = {"RAEA_FOG_REFERENCE": "grid", "RAEA_FOG_MODE": "froxel"} if reference == "grid" else {"RAEA_FOG_REFERENCE": "1"}
    edits = {}
    for name in only:
        edit = source(name, fresh_src)
        edits.setdefault(str(edit), []).append(name)
        frames = SCENES[name][3]
        ref = OUT / ("ref" if reference == "bevy" else f"ref-{reference}") / name
        if fresh_ref or not all((ref / f"{f}.png").exists() for f in frames):
            stills(edit, frames, ref, ref_env)
        cand = OUT / label / name
        stills(edit, frames, cand, env)
        scores = {f: compare(ref / f"{f}.png", cand / f"{f}.png") for f in frames}
        result["scenes"][name] = {"frames": {str(f): {"ssim": s, "psnr": p} for f, (s, p) in scores.items()}, "ssim_min": min(s for s, _ in scores.values()), "psnr_min": min(p for _, p in scores.values())}
    if not no_clip:
        for edit, names in edits.items():
            for name, cost in clip_cost(Path(edit), [n for n in names if SCENES[n][4]], env, OUT / label / "clip" / Path(edit).stem).items():
                result["scenes"][name].update(cost)
    (OUT / f"score-{label}.json").write_text(json.dumps(result, indent=1))
    against = "Bevy's march at 512 undithered steps" if reference == "bevy" else "the grid's model marched per pixel at 1024 steps"
    print(f"fog-score {label} {' '.join(f'{k}={v}' for k, v in env.items())}  (against {against}, at full resolution)")
    print(f"{'scene':20} {'SSIM min':>9} {'PSNR min':>9} {'ms/frame':>9} {'kb/s x265':>10}")
    for name, r in result["scenes"].items():
        print(f"{name:20} {r['ssim_min']:9.5f} {r['psnr_min']:9.2f} {r.get('ms_frame') or '-':>9} {r.get('kbps_x265') or '-':>10}")
    for flake in FLAKES:
        print(f"fog-score: flaky draw, {flake}")
    print(f"written {OUT / f'score-{label}.json'}")


if __name__ == "__main__":
    main()
