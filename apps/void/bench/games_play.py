#!/usr/bin/env python3
"""Every game in the repo played in void, headless: a test instance of void with no window on screen
(RAEA_VOID_HEADLESS=1: its stage paints into its texture and plays), its own home, ports, trace folder and workspace,
started here and killed when done. Nothing shows on the person's screen and nothing takes focus.

  apps/void/bench/games_play.py [--out OUT.json] [--seconds 12] [--only SLUG,...] [--no-lock] [--keep]

The games (GAMES below, the inventory of 2026-10-18): react/bench/games/*, engine/media/golden/50-game, the conformance
goldens' ECS worlds, the playtest fixture, the raea-bevy skill's and void's craft prompts' game examples (taken from
their markdown, so they follow the docs), the game library's games, and the void agents' pongs from the eval runs that
sdk/react/bench/corpus/list.txt names (SKIP where this Mac doesn't have them).

Each one is an object of kind "game" in the workspace (its code copied, everything else linked: footage is never
copied or committed), opened through void's MCP (act open_object) and played SECONDS with the probe pressing arrow keys
at random moments (50-250 ms apart). Then, per game:
  - opens:  the stage's frame moves past 10 and it reports no error;
  - rate:   frames painted at the composition's rate (>= 90% of it) with at most 1% missed (a gap over 1.5 frames);
            the play runs under the shared measurement lock (/tmp/raea-measure.lock, each hold under 10 minutes) and
            the load is recorded beside it;
  - play:   the stage's page where the game is (/snapshot as=page: composited on the GPU as the stage does) against `raea-motion render --frame N --input LOG` of the
            keys each frame carried (/inputlog), SSIM >= 0.999: the same game, 3D included (a game with 3D whose stage
            doesn't draw it fails here);
  - input:  a game that reads input renders differently at frame N with the play's keys than with none pressed
            (SSIM < 0.999 between the two renders);
  - sound:  a game with sound put it through the mixer: the mixer's tap (RAEA_AUDIO_TAP, what went to the device
            before the mute, written when the next object's open drops the mixer) peaks above -60 dBFS;
  - panic:  void is still running and its flight recorder (RAEA_TRACE_DIR/void/<session>/) holds no panic.txt or
            crash.txt.
A PASS or FAIL line per game, the reasons on a FAIL; the numbers in OUT.json. Build first: void (lane build -- cargo
build --release in void/) and raea-motion (react/).
"""
import argparse, glob, json, os, re, shutil, signal, subprocess, sys, time, urllib.parse, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
MOTION = os.path.join(ROOT, "sdk", "react", "target", "release", "raea-motion")
VOID = os.path.join(ROOT, "apps", "void", "target", "release", "void")
LOCK = "/tmp/raea-measure.lock"
# the arrows, two chords, none, and Space (a title screen's "PRESS SPACE")
KEYS = "ArrowRight,ArrowUp,ArrowLeft,ArrowDown,ArrowRight+ArrowUp,,ArrowLeft+ArrowUp, "
# the same game on screen: void draws big text with slightly different glyph edges than a render does (offscreen play
# equals the render exactly), up to ~1% SSIM on a frame of large type; a missing 3D scene or another game state costs
# far more (a 3D game with no 3D drawn: see the RAEA_LIVE_3D=0 run)
PLAY_SSIM = 0.98

# slug, source: a folder with edit.tsx, a single .tsx, or MARKDOWN#N (the N-th ```tsx block that registers a root)
GAMES = [
    ("pong", "sdk/react/bench/games/pong"),
    ("platformer", "sdk/react/bench/games/platformer"),
    ("tones", "sdk/react/bench/games/tones"),
    ("music", "sdk/react/bench/games/music"),
    ("arena3d", "sdk/react/bench/games/arena3d"),
    ("golden-50-game", "engine/media/golden/50-game"),
    ("world-solid", "sdk/motion/conformance/golden/05-world-solid"),
    ("world-entities", "sdk/motion/conformance/golden/06-world-entities"),
    ("ledge-fixed", "sdk/react/bench/playtest/fixtures/ledge-fixed"),
    ("skill-dodge3d", "sdk/skills/raea-bevy/games.md#1"),
    ("skill-hop3d", "sdk/skills/raea-bevy/games.md#2"),
    ("craft-bevy-1", "apps/void/prompts/craft/bevy/games.md#1"),
    ("craft-bevy-2", "apps/void/prompts/craft/bevy/games.md#2"),
    ("craft-motion-1", "apps/void/prompts/craft/motion/games.md#1"),
    ("lib-breakout", "sdk/motion/library/games/tsx/breakout.tsx"),
    ("lib-platformer-feel", "sdk/motion/library/games/tsx/platformer-feel.tsx"),
]


def eval_pongs():
    out = []
    for line in open(os.path.join(ROOT, "sdk/react/bench/corpus/list.txt")):
        line = line.strip()
        if line.startswith("~/") and "/spaces/pong/" in line:
            out.append((f"eval-pong-{len(out) + 1}", os.path.dirname(os.path.expanduser(line))))
    return out


def http(url, body=None, timeout=60):
    req = urllib.request.Request(url, data=json.dumps(body).encode() if body is not None else None,
                                 headers={"Content-Type": "application/json"} if body is not None else {})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        t = r.read().decode()
    return json.loads(t) if t else None


class Void:
    def __init__(self, mcp_port, probe_port):
        self.mcp = f"http://127.0.0.1:{mcp_port}/mcp"
        self.probe = f"http://127.0.0.1:{probe_port}"
        self.id = 0

    def act(self, action, value=None):
        self.id += 1
        r = http(self.mcp, {"jsonrpc": "2.0", "id": self.id, "method": "tools/call",
                            "params": {"name": "act", "arguments": {"action": action, "value": value}}})
        res = r.get("result", {})
        if res.get("isError"):
            raise RuntimeError(f"act {action} {value}: {res['content'][0]['text']}")
        return res

    def p(self, route, **q):
        qs = "&".join(f"{k}={urllib.parse.quote(str(v), safe='')}" for k, v in q.items())
        return http(f"{self.probe}{route}{'?' + qs if qs else ''}", timeout=120)


def md_block(md, n):
    """The n-th ```tsx block of a markdown file that registers a root (a whole composition)."""
    blocks = [b for b in re.findall(r"```tsx\n(.*?)```", open(md).read(), re.S) if "registerRoot(" in b]
    return blocks[n - 1] if n <= len(blocks) else None


def place(slug, src, proj):
    """The object's folder: code copied, everything else linked. None when the source isn't on this Mac."""
    d = os.path.join(proj, "objects", slug)
    os.makedirs(d, exist_ok=True)
    if "#" in src:
        md, n = src.rsplit("#", 1)
        code = md_block(os.path.join(ROOT, md), int(n))
        if code is None:
            return None
        open(os.path.join(d, "edit.tsx"), "w").write(code)
    else:
        path = src if os.path.isabs(src) else os.path.join(ROOT, src)
        if path.endswith(".tsx"):
            if not os.path.exists(path):
                return None
            shutil.copy(path, os.path.join(d, "edit.tsx"))
        else:
            if not os.path.exists(os.path.join(path, "edit.tsx")):
                return None
            for n in os.listdir(path):
                e = os.path.join(path, n)
                if n in ("space.json", "final.mp4", "render.json", "previews") or n.startswith("."):
                    continue
                if n.endswith((".tsx", ".ts")):
                    shutil.copy(e, os.path.join(d, n))
                    unrelative(e, os.path.join(d, n))
                else:
                    os.symlink(e, os.path.join(d, n))
    json.dump({"name": slug, "slug": slug, "created": 0, "opened": 0, "hue": 200, "kind": "game"}, open(os.path.join(d, "space.json"), "w"))
    return os.path.join(d, "edit.tsx")


def unrelative(src, dst):
    """Imports that reach into sdk/motion/src by a relative path (the conformance goldens') as @raea/motion, so the copy
    bundles where it is."""
    here = os.path.dirname(src)
    motion = os.path.join(ROOT, "sdk", "motion", "src")
    def fix(m):
        p = os.path.normpath(os.path.join(here, m.group(2)))
        if p == motion or p.startswith(motion + os.sep):
            return f'{m.group(1)}"@raea/motion{p[len(motion):]}"'
        return m.group(0)
    t = open(dst).read()
    open(dst, "w").write(re.sub(r'(from\s+)"(\.\.?/[^"]*)"', fix, t))


def ungenerated(edit):
    """The game's sounds are all generated ones (sao, lyria) and none was made into its store (assets/made.jsonl beside
    it): a test instance has no generation token, so they can't sound here."""
    d = os.path.dirname(edit)
    t = open(edit).read() + "".join(open(f).read() for f in glob.glob(os.path.join(d, "components", "*.ts*")))
    prompts = re.findall(r'\b(?:sao|lyria)\(\s*"([^"]+)"', t)
    if not prompts or re.search(r"<Audio\s+src=\{?\s*(?:staticFile|\"|')", t):
        return False
    made = "".join(open(f).read() for f in glob.glob(os.path.join(d, "assets", "made.jsonl")))
    return not any(json.dumps(p)[1:-1] in made for p in prompts)


def traits(edit):
    """What the game's source says it does: reads input, sounds, draws 3D."""
    t = open(edit).read()
    for f in glob.glob(os.path.join(os.path.dirname(edit), "components", "*.ts*")):
        t += open(f).read()
    return {
        "input": bool(re.search(r"useInput\(|useGameState\(|useGameEvents\(", t)),
        "sound": bool(re.search(r"<Audio\b|<Music\b|\bsao\(|\blyria\(|@raea/motion/tone|<Video\b", t)),
        "3d": bool(re.search(r"<Canvas3D\b|<Entities\b|@raea/motion/3d", t)),
    }


def lock_take(log):
    t = time.time()
    while True:
        try:
            os.mkdir(LOCK)
            break
        except FileExistsError:
            if time.time() - t > 600:
                raise RuntimeError("the measurement lock wasn't free in 10 minutes")
            time.sleep(5)
    t = time.time()
    while os.getloadavg()[0] >= 6 and time.time() - t < 240:
        log(f"  load {os.getloadavg()[0]:.1f}, waiting for < 6")
        time.sleep(15)
    return time.time()


def lock_give():
    try:
        os.rmdir(LOCK)
    except OSError:
        pass


def ssim(a, b):
    r = subprocess.run(["ffmpeg", "-i", a, "-i", b, "-lavfi", "[0]format=rgb24[a];[1]format=rgb24[b];[a][b]ssim", "-f", "null", "-"], capture_output=True, text=True)
    m = re.findall(r"All:([0-9.]+)", r.stderr)
    return float(m[-1]) if m else None


def size_of(png):
    r = subprocess.run(["sips", "-g", "pixelWidth", "-g", "pixelHeight", png], capture_output=True, text=True).stdout
    w, h = re.search(r"pixelWidth: (\d+)", r), re.search(r"pixelHeight: (\d+)", r)
    return (int(w.group(1)), int(h.group(1))) if w and h else None


def peak_db(wav):
    r = subprocess.run(["ffmpeg", "-i", wav, "-af", "volumedetect", "-f", "null", "-"], capture_output=True, text=True)
    m = re.search(r"max_volume: (-?[0-9.]+|-inf) dB", r.stderr)
    return None if not m else (-200.0 if m.group(1) == "-inf" else float(m.group(1)))


def render(edit, f, inputs, out, stage=None):
    cmd = ["nice", "-n", "15", MOTION, "render", edit, "-o", out, "--frame", str(f), "--input", inputs]
    if stage:
        cmd += ["--stage", f"{stage[0]}x{stage[1]}"]
    r = subprocess.run(cmd, env={**os.environ, "RAEA_READBACK": "rgba"}, capture_output=True, text=True)
    if r.returncode != 0 or not os.path.exists(out):
        return f"render failed: {(r.stderr or r.stdout).strip().splitlines()[-1:] or '?'}"
    return None


def wait_open(v, slug, timeout=150):
    t = time.time()
    last = None
    while time.time() - t < timeout:
        r = v.p("/report")
        f = r.get("stage_frame", -1)
        if r.get("stage_error") and time.time() - t > 10:
            raise RuntimeError(r["stage_error"])
        if last is not None and f > last and f > 10:
            return round(time.time() - t, 1), r
        last = f
        time.sleep(0.5)
    raise RuntimeError("the stage never played")


def faults(trace):
    # the recorder makes crash.txt empty at start, for a hard crash to be written into: only a report counts
    return [p for p in glob.glob(os.path.join(trace, "void", "*", "*")) if os.path.basename(p) in ("panic.txt", "crash.txt") and os.path.getsize(p) > 0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=os.path.join(os.environ.get("TMPDIR", "/tmp"), "raea-void-games", "games.json"))
    ap.add_argument("--seconds", type=float, default=12)
    ap.add_argument("--only", default=None, help="slugs, comma-separated")
    ap.add_argument("--mcp-port", type=int, default=int(os.environ.get("RAEA_VOID_MCP_PORT", 7487)))
    ap.add_argument("--probe-port", type=int, default=int(os.environ.get("RAEA_VOID_PROBE_PORT", 7488)))
    ap.add_argument("--no-lock", action="store_true", help="play without the measurement lock (the rate is then not judged)")
    ap.add_argument("--keep", action="store_true", help="keep the workspace, taps and shots")
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the void instance (RAEA_LIVE_3D=0 is the old stage)")
    ap.add_argument("--export", action="store_true", help="the games exported (raea-motion export-game, mac-app and mac-cli) and their players played headless instead of void")
    a = ap.parse_args()
    out_dir = os.path.dirname(os.path.abspath(a.out))
    proj, home, shots, taps, trace = (os.path.join(out_dir, n) for n in ("proj", "home", "shots", "taps", "trace"))
    for d in (proj, home, shots, taps, trace):
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(d)
    os.makedirs(os.path.join(proj, "objects"))
    # a workspace has a brand (void makes one): the games' var(--brand-*) colours come from it
    os.makedirs(os.path.join(proj, "brand"))
    shutil.copy(os.path.join(ROOT, "sdk/react/bench/games/pong/brand/brand.css"), os.path.join(proj, "brand", "brand.css"))
    log = lambda s: print(s, flush=True)
    games = GAMES + eval_pongs()
    if a.only:
        want = set(a.only.split(","))
        games = [g for g in games if g[0] in want]
    results = {"started": time.strftime("%Y-%m-%d %H:%M:%S"), "seconds": a.seconds, "env": a.env, "games": {}}
    placed = []
    seen_code = {}
    for slug, src in games:
        edit = place(slug, src, proj)
        if edit is None:
            results["games"][slug] = {"source": src, "verdict": "SKIP", "why": ["not on this Mac"]}
            continue
        code = open(edit).read()
        if code in seen_code:
            # the same example in two docs: played once
            shutil.rmtree(os.path.dirname(edit))
            results["games"][slug] = {"source": src, "verdict": "SAME", "why": [f"the same code as {seen_code[code]}"]}
            continue
        seen_code[code] = slug
        placed.append((slug, src, edit))
    # the object opened last, so the last game's mixer is dropped (and its tap written)
    end = os.path.join(proj, "objects", "zz-end")
    os.makedirs(end)
    open(os.path.join(end, "edit.tsx"), "w").write('import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "@raea/motion";\n'
        'function End() { const f = useCurrentFrame(); return <AbsoluteFill style={{ background: "#111", color: "#eee", fontSize: 40 }}>{`end ${f}`}</AbsoluteFill>; }\n'
        'registerRoot(() => <Composition id="end" component={End} width={640} height={360} fps={30} durationInFrames={300} />);\n')
    json.dump({"name": "zz-end", "slug": "zz-end", "created": 0, "opened": 0, "hue": 0, "kind": "game"}, open(os.path.join(end, "space.json"), "w"))

    if a.export:
        return export_all(a, placed, results, out_dir, shots, log)

    for port in (a.mcp_port, a.probe_port):
        if subprocess.run(["lsof", f"-iTCP:{port}", "-sTCP:LISTEN"], capture_output=True).returncode == 0:
            sys.exit(f"port {port} is taken: pass --mcp-port/--probe-port (or RAEA_VOID_MCP_PORT/RAEA_VOID_PROBE_PORT)")
    tap = os.path.join(taps, "current.wav")
    env = {**os.environ, "RAEA_VOID_HOME": home, "RAEA_VOID_MCP_PORT": str(a.mcp_port), "RAEA_VOID_PROBE": "1",
           "RAEA_VOID_PROBE_PORT": str(a.probe_port), "RAEA_VOID_HEADLESS": "1", "RAEA_VOID_BACKGROUND": "1",
           "RAEA_AUDIO_MUTE": "1", "RAEA_AUDIO_TAP": tap, "RAEA_TRACE_DIR": trace}
    for kv in a.env:
        k, v = kv.split("=", 1)
        env[k] = v
    vlog = open(os.path.join(out_dir, "void.log"), "w")
    proc = subprocess.Popen(["nice", "-n", "15", VOID, proj], env=env, stdout=vlog, stderr=subprocess.STDOUT, start_new_session=True)

    def stop():
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
    signal.signal(signal.SIGTERM, lambda *_: (stop(), lock_give(), sys.exit(143)))
    v = Void(a.mcp_port, a.probe_port)
    held = None
    try:
        t = time.time()
        while time.time() - t < 90:
            try:
                v.p("/report")
                break
            except Exception:
                time.sleep(1)
        else:
            raise RuntimeError("void's probe never answered (see void.log)")
        prev = None
        for slug, src, edit in placed + [("zz-end", None, os.path.join(end, "edit.tsx"))]:
            res = results["games"].setdefault(slug, {"source": src}) if src else {}
            if src:
                res.update(traits(edit))
            # the lock held in stretches under 10 minutes, the load recorded
            if not a.no_lock and src and (held is None or time.time() - held > 540):
                if held:
                    lock_give()
                held = lock_take(log)
            log(f"{slug}: opening")
            try:
                v.act("open_object", slug)
                opened = wait_open(v, slug)
            except Exception as e:
                opened = None
                if src:
                    res["error"] = f"open: {e}"[:400]
            # the game before's sound: its mixer was dropped by this open, which wrote its tap
            if prev:
                time.sleep(0.5)
                if os.path.exists(tap):
                    shutil.move(tap, os.path.join(taps, f"{prev}.wav"))
            prev = slug if src else None
            if not src:
                break
            if opened is None:
                if proc.poll() is not None:
                    res["error"] = (res.get("error", "") + f"; void exited ({proc.returncode})").strip("; ")
                    break
                continue
            res["opened_s"] = opened[0]
            v.p("/reset")
            v.p("/input", keys=KEYS, seconds=a.seconds)
            load0 = os.getloadavg()[0]
            time.sleep(a.seconds + 0.5)
            rep = v.p("/report")
            res["load1"] = [round(load0, 1), round(os.getloadavg()[0], 1)]
            st = rep["stage"]
            res["measured"] = {"fps": rep.get("stage_rate") or rep.get("composition_fps"), "painted_fps": round(st["painted_fps"], 1),
                               "painted": st["painted"], "missed": st["missed"], "work_p99_ms": st["work_ms"]["p99"],
                               "drift_samples": rep.get("drift_ms", {}).get("samples"), "rss_max_mb": rep.get("rss_max_mb"),
                               "stage_error": rep.get("stage_error"), "page": rep.get("page")}
            png = os.path.join(shots, f"{slug}-void.png")
            # the page composited on the GPU as the stage composites it (its 3D under it): what the window shows, at the
            # page's own size; the CPU's paint of the page where the probe has no as=page
            s = v.p("/snapshot", path=png, **{"as": "page"})
            if "error" in s or s.get("frame") is None:
                s = v.p("/snapshot", path=png)
            inputs = os.path.join(shots, f"{slug}-input.json")
            v.p("/inputlog", path=inputs)
            res["snapshot"] = s
            res["faults"] = faults(trace)
            # judged as the store stood after the play (the renders below make what a render makes)
            res["ungenerated"] = ungenerated(edit)
            if proc.poll() is not None:
                res["error"] = f"void exited ({proc.returncode})"
                break
            log(f"  {res['measured']['painted_fps']} of {res['measured']['fps']} fps, missed {st['missed']}/{st['painted']}, work p99 {st['work_ms']['p99']:.1f} ms, load {res['load1']}")
    finally:
        if held:
            lock_give()
        stop()
    results["void_exit"] = proc.returncode
    results["faults"] = faults(trace)

    # render equals play, input, sound: no timing in these, so no lock
    for slug, src, edit in placed:
        res = results["games"][slug]
        why = []
        if res.get("error"):
            why.append(res["error"])
        m = res.get("measured")
        s = res.get("snapshot") or {}
        if m:
            if m.get("stage_error"):
                why.append(f"stage error: {m['stage_error'][:200]}")
            fps = m["fps"] or 30
            res["rate_judged"] = not a.no_lock
            if not a.no_lock and (m["painted_fps"] < 0.9 * fps or m["missed"] > max(1, 0.01 * m["painted"])):
                why.append(f"rate: {m['painted_fps']} of {fps} fps painted, {m['missed']} missed")
        if s.get("frame") is not None and "error" not in s:
            f = s["frame"]
            inputs = os.path.join(shots, f"{slug}-input.json")
            ren = os.path.join(shots, f"{slug}-render-{f}.png")
            play = os.path.join(shots, f"{slug}-void.png")
            err = render(edit, f, inputs, ren)
            if not err and size_of(ren) != size_of(play):
                # a responsive game lays out at void's stage box: rendered at that size
                err = render(edit, f, inputs, ren, stage=size_of(play))
            if err:
                why.append(err)
            else:
                res["ssim_play"] = ssim(ren, play)
                if res["ssim_play"] is None or res["ssim_play"] < PLAY_SSIM:
                    why.append(f"play != render at frame {f} (SSIM {res['ssim_play']}){' (its 3D?)' if res.get('3d') else ''}")
                log_in = json.load(open(inputs))
                pressed = sum(1 for e in log_in.values() if e.get("keys"))
                res["frames_with_keys"] = [pressed, len(log_in)]
                if res.get("input") and pressed:
                    none = os.path.join(shots, f"{slug}-nokeys.json")
                    json.dump({k: {"keys": [], "live": True} for k in log_in}, open(none, "w"))
                    ren0 = os.path.join(shots, f"{slug}-render-nokeys-{f}.png")
                    err = render(edit, f, none, ren0, stage=size_of(play) if size_of(ren) != size_of(play) else None)
                    if err:
                        why.append("no-keys " + err)
                    else:
                        res["ssim_vs_nokeys"] = ssim(ren, ren0)
                        if res["ssim_vs_nokeys"] is not None and res["ssim_vs_nokeys"] >= 0.999:
                            why.append("input changed nothing on screen")
        elif m:
            why.append(f"no snapshot: {s.get('error')}")
        wav = os.path.join(taps, f"{slug}.wav")
        res["tap_peak_db"] = peak_db(wav) if os.path.exists(wav) else None
        if res.get("sound") and m and (res["tap_peak_db"] is None or res["tap_peak_db"] < -60):
            if res.get("ungenerated"):
                res["sound_note"] = "its sounds are generated (sao/lyria) and none is made in its store: not judged here"
            else:
                why.append(f"no sound through the mixer (tap peak {res['tap_peak_db']})")
        if res.get("faults"):
            why.append(f"panic/crash: {res['faults']}")
        res["verdict"] = "FAIL" if why else "PASS"
        res["why"] = why
    json.dump(results, open(a.out, "w"), indent=1)
    for slug, res in results["games"].items():
        m = res.get("measured") or {}
        bits = []
        if m:
            bits.append(f"{m.get('painted_fps')}/{m.get('fps')} fps, missed {m.get('missed')}, p99 {m.get('work_p99_ms') or 0:.1f} ms")
        if res.get("ssim_play") is not None:
            bits.append(f"play~render {res['ssim_play']:.4f}")
        if res.get("ssim_vs_nokeys") is not None:
            bits.append(f"keys~none {res['ssim_vs_nokeys']:.3f}")
        if res.get("tap_peak_db") is not None:
            bits.append(f"tap {res['tap_peak_db']:.0f} dB" + (" (sounds not generated here)" if res.get("sound_note") else ""))
        flags = "".join(k[0] for k in ("3d", "sound", "input") if res.get(k))
        line = f"{res['verdict']} {slug} [{flags}] " + "; ".join(bits)
        if res.get("why"):
            line += "  -- " + " | ".join(str(w) for w in res["why"])
        print(line)
    print(f"results: {a.out}")
    if not a.keep:
        shutil.rmtree(proj, ignore_errors=True)
        shutil.rmtree(home, ignore_errors=True)
    sys.exit(0 if all(r["verdict"] in ("PASS", "SKIP", "SAME") for r in results["games"].values()) else 1)


GAME_BIN = os.path.join(ROOT, "sdk", "react", "target", "release", "raea-game")
SNAPS = (2.5, 8.0)


def no_js_engines(exe):
    """What the player links and holds that would be bun, node or V8 (none expected: the game runs on JavaScriptCore)."""
    libs = subprocess.run(["otool", "-L", exe], capture_output=True, text=True).stdout
    bad = [l.strip() for l in libs.splitlines()[1:] if re.search(r"v8|node|bun", l, re.I)]
    strs = subprocess.run(["sh", "-c", f"strings -a '{exe}' | grep -c -E 'v8::Isolate|node::Environment|Bun\\.serve'"], capture_output=True, text=True).stdout.strip()
    if strs not in ("", "0"):
        bad.append(f"{strs} V8/node/bun symbols in the binary")
    return bad


def export_all(a, placed, results, out_dir, shots, log):
    """--export: each game exported as a mac app and a mac CLI folder, copied away from the repo, its player's self-test run
    with nothing of the dev tree around it (env -i, PATH=/usr/bin:/bin, HOME empty): offscreen, the probe's random keys,
    the sound mixed silently. Then its snapshots against the dev tree's render of the logged input, a no-keys render for
    input, the self-test's mixer peak for sound, and the player checked for bun, node and V8."""
    import tempfile
    work = tempfile.mkdtemp(prefix="raea-game-export-", dir=os.environ.get("TMPDIR", "/tmp"))
    rows = []
    for slug, src, edit in placed:
        tr = traits(edit)
        # styles.gen.ts made as void makes it before it opens or renders an object (host::styles)
        d = os.path.dirname(edit)
        if not os.path.exists(os.path.join(d, "styles.gen.ts")) and any("styles.gen" in open(f).read() for f in glob.glob(os.path.join(d, "*.ts*")) + glob.glob(os.path.join(d, "components", "*.ts*"))):
            css = subprocess.run(["bun", os.path.join(ROOT, "apps/void/tw/project.ts"), d, os.path.join(ROOT, "apps/cli/template/theme.css")], capture_output=True, text=True).stdout
            open(os.path.join(d, "styles.gen.ts"), "w").write("// written by raea render: the brand's variables and the project's Tailwind\nexport const css = " + json.dumps(css) + ";\n")
        for target in ("mac-app", "mac-cli"):
            name = f"{slug}-{target}"
            res = results["games"].setdefault(name, {"source": src, "target": target, **tr})
            why = []
            out = os.path.join(work, name, slug + (".app" if target == "mac-app" else ""))
            os.makedirs(os.path.dirname(out), exist_ok=True)
            t = time.time()
            r = subprocess.run(["nice", "-n", "15", MOTION, "export-game", edit, "-o", out, "--target", target, "--keys", KEYS], capture_output=True, text=True)
            said = r.stdout + r.stderr
            if "EXPORTED" not in said:
                res.update(verdict="FAIL", why=["export: " + (next((l for l in said.splitlines() if "REFUSED" in l), said.strip().splitlines()[-1:] or ["?"]))[:300]])
                log(f"{name}: export failed")
                continue
            res["export_s"] = round(time.time() - t, 1)
            try:
                ex = json.loads(said.split("EXPORTED", 1)[1].strip().splitlines()[0] if said.split("EXPORTED", 1)[1].strip().startswith("{") else said.split("EXPORTED", 1)[1])
            except Exception:
                ex = {}
            res["size_mb"] = ex.get("total_mb")
            run = os.path.join(work, "run-" + name)
            os.makedirs(os.path.join(run, "home"))
            os.makedirs(os.path.join(run, "tmp"))
            dst = os.path.join(run, os.path.basename(out))
            subprocess.run(["cp", "-R", out, dst])
            if target == "mac-app":
                mac = os.path.join(dst, "Contents", "MacOS")
                exe = os.path.join(mac, os.listdir(mac)[0])
                gj = os.path.join(dst, "Contents", "Resources", "game", "game.json")
            else:
                exe = next((os.path.join(dst, f) for f in os.listdir(dst) if os.access(os.path.join(dst, f), os.X_OK) and os.path.isfile(os.path.join(dst, f))), None)
                gj = os.path.join(dst, "game", "game.json")
            if not exe:
                res.update(verdict="FAIL", why=["no player in the export"])
                continue
            res["size_mb"] = res.get("size_mb") or round(int(subprocess.run(["du", "-sk", dst], capture_output=True, text=True).stdout.split()[0]) / 1024, 1)
            res["js_engines"] = no_js_engines(exe)
            if res["js_engines"]:
                why.append(f"links or holds a JS engine: {res['js_engines']}")
            fps = json.load(open(gj)).get("fps", 30) if os.path.exists(gj) else 30
            snaps = []
            for sec in SNAPS:
                snaps += ["--snapshot", str(sec), os.path.join(run, f"snap-{sec}.png")]
            t = time.time()
            r = subprocess.run(["env", "-i", f"HOME={run}/home", "PATH=/usr/bin:/bin", f"TMPDIR={run}/tmp", "nice", "-n", "15", exe, "--selftest", f"{SNAPS[-1] + 1}s",
                                "--keys", KEYS, "--input-log", os.path.join(run, "input.json"), "--json", os.path.join(run, "selftest.json")] + snaps,
                               cwd=run, capture_output=True, text=True)
            res["selftest_wall_s"] = round(time.time() - t, 1)
            # launched again with the same home: the first launch compiles the GPU's shaders into an empty cache (a
            # 3D game's first open took 5 s, its second 0.45 s)
            subprocess.run(["env", "-i", f"HOME={run}/home", "PATH=/usr/bin:/bin", f"TMPDIR={run}/tmp", "nice", "-n", "15", exe, "--selftest", "1s",
                            "--json", os.path.join(run, "warm.json")], cwd=run, capture_output=True, text=True)
            if os.path.exists(os.path.join(run, "warm.json")):
                res["open_ms_warm"] = json.load(open(os.path.join(run, "warm.json"))).get("open_ms")
            verdict = (r.stdout.strip().splitlines() or ["?"])[-1]
            st = json.load(open(os.path.join(run, "selftest.json"))) if os.path.exists(os.path.join(run, "selftest.json")) else {}
            res["selftest"] = verdict
            res["open_ms"] = st.get("open_ms")
            res["frames"] = st.get("frames")
            res["load1"] = round(os.getloadavg()[0], 1)
            snd = st.get("sound", {})
            res["sound_peak_dbfs"] = snd.get("peak_dbfs")
            if st.get("panics"):
                why.append(f"{st['panics']} panics")
            if not st:
                why.append(f"the self-test said: {verdict[:200]} {r.stderr.strip().splitlines()[-1:] if r.stderr.strip() else ''}")
            # sound: mixed by the player's own mixer (sounds only generated with a model aren't packaged unless made)
            if tr["sound"] and st and not snd.get("mixed"):
                if ungenerated(edit):
                    res["sound_note"] = "its sounds are generated and none is made in its store: not judged"
                else:
                    why.append("the game has sound and the player mixed none")
            # play = render of the logged input; input: the same frame with no keys pressed differs
            inputs = os.path.join(run, "input.json")
            res["ssim"] = []
            staged = False
            for sec in SNAPS:
                play = os.path.join(run, f"snap-{sec}.png")
                f = round(sec * fps)
                if not os.path.exists(play):
                    why.append(f"no snapshot at {sec} s")
                    continue
                ren = os.path.join(run, f"render-{sec}.png")
                err = render(edit, f, inputs, ren)
                if not err and size_of(ren) != size_of(play):
                    err = render(edit, f, inputs, ren, stage=size_of(play))
                    staged = True
                if err:
                    why.append(err)
                    continue
                v = ssim(ren, play)
                res["ssim"].append(v)
                if v is None or v < PLAY_SSIM:
                    why.append(f"player != render at frame {f} (SSIM {v})")
            if tr["input"] and os.path.exists(inputs) and res["ssim"]:
                log_in = json.load(open(inputs))
                f = round(SNAPS[-1] * fps)
                none = os.path.join(run, "nokeys.json")
                json.dump({k: {"keys": [], "live": True} for k in log_in}, open(none, "w"))
                ren0 = os.path.join(run, "render-nokeys.png")
                play = os.path.join(run, f"snap-{SNAPS[-1]}.png")
                # at the size the play's frame was rendered at (a responsive game's stage)
                if not render(edit, f, none, ren0, stage=size_of(play) if size_of(os.path.join(run, f"render-{SNAPS[-1]}.png")) == size_of(play) and staged else None):
                    res["ssim_vs_nokeys"] = ssim(os.path.join(run, f"render-{SNAPS[-1]}.png"), ren0)
                    if res["ssim_vs_nokeys"] is not None and res["ssim_vs_nokeys"] >= 0.999:
                        why.append("input changed nothing on screen")
            if not verdict.startswith("PASS") and not why:
                why.append("self-test: " + verdict[:200])
            res["verdict"] = "FAIL" if why else "PASS"
            res["why"] = why
            log(f"{name}: {res['verdict']} {' | '.join(why)}")
    json.dump(results, open(a.out, "w"), indent=1)
    print(f"{'game':24s} {'target':8s} opens input play~render sound     size_MB open_ms(first/again) load verdict")
    for name, res in results["games"].items():
        if "target" not in res:
            continue
        ss = [x for x in (res.get("ssim") or []) if x is not None]
        opens = "yes" if res.get("frames") else "no"
        inp = "n/a" if not res.get("input") else ("yes" if res.get("ssim_vs_nokeys") is not None and res["ssim_vs_nokeys"] < 0.999 else "no")
        snd = "n/a" if not res.get("sound") else ("%.0f dBFS" % res["sound_peak_dbfs"] if res.get("sound_peak_dbfs") is not None and res.get("sound_peak_dbfs") > -120 else ("not made" if res.get("sound_note") else "none"))
        print(f"{name.rsplit('-mac', 1)[0]:24s} {res['target']:8s} {opens:5s} {inp:5s} {('%.4f' % min(ss)) if ss else '-':11s} {snd:9s} {res.get('size_mb')!s:7s} {str(res.get('open_ms')) + '/' + str(res.get('open_ms_warm')):20s} {res.get('load1')!s:4s} {res.get('verdict')}{'  -- ' + ' | '.join(res.get('why') or []) if res.get('why') else ''}")
    if not a.keep:
        shutil.rmtree(work, ignore_errors=True)
    sys.exit(0 if all(r.get("verdict") in ("PASS", "SKIP", "SAME") for r in results["games"].values()) else 1)


if __name__ == "__main__":
    main()
