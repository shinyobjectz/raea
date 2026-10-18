#!/usr/bin/env python3
"""Does the Bevy skills library teach? GLM 5.3 Flash (OpenRouter) writes one composition per 3D or game brief, with a
`craft` tool to read the craft files, in two conditions:
  before: the motion files as they were before the library (motion.md, motion/3d and the other motion/* topics,
          from git rev BEFORE_REV)
  after:  the same, as they are now, plus the bevy router (in the system prompt) and its topics behind the tool
Each first answer is rendered at its first, middle and last frame with raea-motion. A first-try success renders all
three frames, its middle frame isn't blank, and its source uses what the brief asks for (a regex per brief).
Tokens and cost come from OpenRouter's usage. Writes OUT/results.json and prints the table.
   run.py [--samples N] [--out DIR] [--only before|after]
"""
import concurrent.futures as cf, json, os, pathlib, re, subprocess, sys, time, urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[4]
MODEL = "z-ai/glm-5.3-flash"
PRICE_IN, PRICE_OUT = 0.048e-6, 0.20e-6  # $ per token (OpenRouter, 2026-10-18)
BEFORE_REV = "d565763"  # the rows path landed, the SDK and the library not yet
BIN = ROOT / "react/target/release/raea-motion"
ARGS = sys.argv[1:]
SAMPLES = int(ARGS[ARGS.index("--samples") + 1]) if "--samples" in ARGS else 3
OUT = pathlib.Path(ARGS[ARGS.index("--out") + 1]) if "--out" in ARGS else pathlib.Path(os.environ.get("TMPDIR", "/tmp")) / "raea-bevy-eval"
ONLY = ARGS[ARGS.index("--only") + 1] if "--only" in ARGS else None

BRIEFS = [
    ("product", "A 4-second product shot, 1080x1080 at 30 fps: a perfume bottle (model it from simple shapes) on a "
     "turntable that turns slowly, lit by a soft spotlight from above that casts a shadow on the floor, while the "
     "camera pushes in. The words \"Night Bloom\" sit over the top of the frame.",
     [r"SpotLight", r"castShadow", r"PerspectiveCamera"]),
    ("neon", "A 3-second 1280x720 loop at 30 fps: the word OPEN as glowing neon letters in 3D, the glow blooming, "
     "drifting fog behind it, the letters flickering on two fixed frames.",
     [r"<Bloom", r"emissive", r"Text3D|text3d", r"<Fog"]),
    ("model", "A 2-second 1280x720 shot at 30 fps of the 3D model media/model.glb (it has an animation clip named "
     "\"Spin\") playing its clip on a floor, with the picture media/poster.png standing beside it as a sign in the scene.",
     [r"<Model[^>]*model\.glb", r"animation=", r"<ImagePlane|poster\.png"]),
    ("game", "A 5-second 1280x720 scripted play-through at 30 fps of a 3D game: the player steers a ball left and right "
     "along a lane to dodge blocks coming towards the camera; the blocks spawn in a seeded pattern; the score (blocks "
     "passed) is shown at the top. The steering comes from a scripted input timeline in the file.",
     [r"useGameState", r"<Mesh", r"Canvas3D"]),
    ("finishes", "A 2-second 1280x720 shot at 30 fps of three spheres in a row: one of clear glass you can see through, "
     "one in glossy lacquered car paint with a clear coat, and one whose colour shifts from teal to orange over the shot.",
     [r"transparent|opacity", r"clearcoat", r"color=\{"]),
    ("city", "A 4-second 1080x1080 shot at 30 fps: a small city of box buildings on a grid that rise out of the ground "
     "one after another, the camera orbiting the city once, a sun casting shadows; a beacon sphere appears on the "
     "tallest building after 2 seconds.",
     [r"Sequence|>\s*\d+\s*&&|frame\s*>=?\s*\d+", r"Math\.(sin|cos)", r"DirectionalLight"]),
]

ROLE = ("You write one complete composition file (edit.tsx) for raea's TypeScript SDK @raea/motion, which renders "
        "React-style video compositions; its 3D is drawn by raea's engine with Bevy. Read the craft files you need "
        "with the craft tool before writing. When you are done, answer with the whole file in one ```tsx block: "
        "imports from \"@raea/motion\" and \"@raea/motion/3d\", a component, and registerRoot(() => <Composition ... />). "
        "No other files exist except the media files a brief names. There is no brand for these pieces: choose the "
        "colours yourself as hex strings.")


def craft_files(cond):
    """name -> text for the craft tool, and the system prompt's router text."""
    names = ["motion", "motion/compositions", "motion/timing", "motion/animating", "motion/assets", "motion/3d", "motion/games", "motion/rendering"]
    if cond == "after":
        names += ["bevy"] + [f"bevy/{t}" for t in ["scene", "lights", "materials", "meshes", "animation", "games", "assets", "post", "limits"]]
    files = {}
    for n in names:
        rel = f"void/prompts/craft/{n}.md"
        if cond == "before":
            files[n] = subprocess.run(["git", "-C", str(ROOT), "show", f"{BEFORE_REV}:{rel}"], capture_output=True, text=True, check=True).stdout
        else:
            files[n] = (ROOT / rel).read_text()
    router = files["motion"] + ("\n\n" + files["bevy"] if cond == "after" else "")
    return files, router


def key():
    for line in (pathlib.Path.home() / ".raea/openrouter.env").read_text().splitlines():
        if line.startswith("OPENROUTER_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit("no OPENROUTER_API_KEY")


def chat(messages, tools):
    body = {"model": MODEL, "messages": messages, "tools": tools, "max_tokens": 65536, "temperature": 0.7}
    req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=json.dumps(body).encode(),
                                 headers={"Authorization": f"Bearer {key()}", "Content-Type": "application/json"})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=600) as r:
                return json.load(r)
        except Exception as e:  # network or provider hiccup: retried, never counted as the model's failure
            err = e
            time.sleep(5 * (attempt + 1))
    raise RuntimeError(f"OpenRouter failed: {err}")


def write(cond, brief, k):
    files, router = craft_files(cond)
    tools = [{"type": "function", "function": {"name": "craft", "description": "Read a craft file by name.",
              "parameters": {"type": "object", "properties": {"name": {"type": "string", "enum": sorted(files)}}, "required": ["name"]}}}]
    msgs = [{"role": "system", "content": ROLE + "\n\n" + router}, {"role": "user", "content": brief[1]}]
    usage = {"in": 0, "out": 0, "reads": []}
    for _ in range(16):
        r = chat(msgs, tools)
        u = r.get("usage", {})
        usage["in"] += u.get("prompt_tokens", 0)
        usage["out"] += u.get("completion_tokens", 0)
        m = r["choices"][0]["message"]
        calls = m.get("tool_calls") or []
        msgs.append({k2: v for k2, v in m.items() if k2 in ("role", "content", "tool_calls")})
        if not calls:
            return m.get("content") or "", usage
        for c in calls:
            try:
                name = json.loads(c["function"]["arguments"] or "{}").get("name", "")
            except json.JSONDecodeError:
                name = ""
            usage["reads"].append(name)
            msgs.append({"role": "tool", "tool_call_id": c["id"], "content": files.get(name, f"no craft file {name!r}; one of {', '.join(sorted(files))}")})
    return "", usage


def frames_of(src):
    m = re.search(r"durationInFrames=\{(\d+)\}", src) or re.search(r"durationInFrames:\s*(\d+)", src)
    if m:
        return int(m.group(1))
    m = re.search(r"const\s+\w*(?:FRAMES|DURATION)\w*\s*=\s*(\d+)", src)
    return int(m.group(1)) if m else 60


def luma_range(png):
    out = subprocess.run(["ffmpeg", "-v", "error", "-i", str(png), "-vf", "signalstats,metadata=print:file=-", "-f", "null", "-"], capture_output=True, text=True).stdout
    hi = re.search(r"YMAX=(\d+)", out)
    lo = re.search(r"YMIN=(\d+)", out)
    return int(hi.group(1)) - int(lo.group(1)) if hi and lo else 0


def judge(cond, brief, k, answer):
    d = OUT / f"{cond}-{brief[0]}-{k}"
    (d / "media").mkdir(parents=True, exist_ok=True)
    (d / "media/model.glb").write_bytes((ROOT / "motion/bench/bevy/model/media/spinner.glb").read_bytes())
    (d / "media/poster.png").write_bytes((ROOT / "motion/bench/bevy/planes/media/poster.png").read_bytes())
    (d / "answer.md").write_text(answer)
    blocks = re.findall(r"```(?:tsx|ts|jsx|typescript)?\n(.*?)```", answer, re.S)
    src = next((b for b in blocks if "registerRoot" in b), None)
    res = {"cond": cond, "brief": brief[0], "sample": k, "code": src is not None}
    if src is None:
        res.update(ok=False, why="no file with registerRoot")
        return res
    (d / "edit.tsx").write_text(src)
    res["features"] = [bool(re.search(p, src)) for p in brief[2]]
    total = frames_of(src)
    for f in [0, total // 2, max(0, total - 1)]:
        p = subprocess.run(["nice", "-n", "15", str(BIN), "render", str(d / "edit.tsx"), "--frame", str(f), "-o", str(d / f"f{f}.png")], capture_output=True, text=True, timeout=300)
        if p.returncode != 0 or not (d / f"f{f}.png").exists():
            lines = (p.stdout + p.stderr).splitlines()
            last = [l for l in lines if l.startswith("REFUSED")] or [l for l in lines if l.strip() and "WARN" not in l and "logger" not in l]
            res.update(ok=False, renders=False, why=f"frame {f}: {last[-1][:300] if last else 'no output'}")
            return res
    res["renders"] = True
    res["blank"] = luma_range(d / f"f{total // 2}.png") < 8
    res["ok"] = res["renders"] and not res["blank"] and all(res["features"])
    if not res["ok"]:
        res["why"] = "blank middle frame" if res["blank"] else f"missing: {[p for p, h in zip(brief[2], res['features']) if not h]}"
    return res


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    conds = [ONLY] if ONLY else ["before", "after"]
    jobs = [(c, b, k) for c in conds for b in BRIEFS for k in range(SAMPLES)]
    answers = {}
    with cf.ThreadPoolExecutor(8) as ex:
        futs = {ex.submit(write, c, b, k): (c, b, k) for c, b, k in jobs}
        for fu in cf.as_completed(futs):
            c, b, k = futs[fu]
            try:
                answers[(c, b[0], k)] = fu.result()
            except Exception as e:
                answers[(c, b[0], k)] = ("", {"in": 0, "out": 0, "reads": [], "error": str(e)})
            print(f"wrote {c} {b[0]} {k}", file=sys.stderr)
    results = []
    for c, b, k in jobs:  # renders one at a time (the GPU)
        answer, usage = answers[(c, b[0], k)]
        r = judge(c, b, k, answer)
        r.update(tokens_in=usage["in"], tokens_out=usage["out"], reads=usage["reads"], error=usage.get("error"))
        results.append(r)
        print(f"{c:6} {b[0]:9} {k}  {'OK ' if r['ok'] else 'no '} {r.get('why', '')}", file=sys.stderr)
    tin = sum(r["tokens_in"] for r in results)
    tout = sum(r["tokens_out"] for r in results)
    summary = {}
    for c in conds:
        rs = [r for r in results if r["cond"] == c]
        summary[c] = {"runs": len(rs), "first_try_ok": sum(r["ok"] for r in rs), "renders": sum(bool(r.get("renders")) for r in rs),
                      "by_brief": {b[0]: sum(r["ok"] for r in rs if r["brief"] == b[0]) for b in BRIEFS},
                      "avg_reads": round(sum(len(r["reads"]) for r in rs) / max(1, len(rs)), 1)}
    out = {"model": MODEL, "samples": SAMPLES, "summary": summary, "tokens_in": tin, "tokens_out": tout,
           "cost_usd": round(tin * PRICE_IN + tout * PRICE_OUT, 4), "results": results}
    (OUT / "results.json").write_text(json.dumps(out, indent=1))
    print(json.dumps({k: v for k, v in out.items() if k != "results"}, indent=1))


if __name__ == "__main__":
    main()
