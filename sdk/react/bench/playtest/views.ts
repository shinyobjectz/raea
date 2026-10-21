// Playtest views, rendered (sdk/react/docs/playtest.md): EDIT.playtest.ts's views evaluated over a run that run.ts wrote,
// each instance rendered by the engine from the run's input log, clean or with the game's debug overlays, into the
// media a check pack's questions name. Writes OUT/views/ and OUT/views.json (the manifest).
//   bun sdk/react/bench/playtest/views.ts EDIT.tsx OUT [--only NAME,NAME]
// The overlays reach the render as globalThis.__RAEA_OVERLAYS__, set by a module imported ahead of the edit; a clean view
// renders the edit itself, so the two differ only by the overlays.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fpsOf, loadModule } from "./load";
import type { Moment, PlaytestSpec, TraceRow, View } from "../../../motion/src/playtest";

const [edit, out] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const onlyAt = process.argv.indexOf("--only");
const only = onlyAt > 0 ? process.argv[onlyAt + 1].split(",") : null;
const REACT = path.resolve(import.meta.dir, "../..");
const BIN = path.join(REACT, "target/release/raea-motion");
const FPS = fpsOf(edit); // the game's own rate: a clip plays at it

const spec: PlaytestSpec = (await loadModule(edit.replace(/\.tsx$/, ".playtest.ts"))).default;
const views: View[] = spec.views;
const trace: TraceRow[] = fs.readFileSync(path.join(out, "trace.jsonl"), "utf8").trim().split("\n").map((l) => JSON.parse(l));
const moments = JSON.parse(fs.readFileSync(path.join(out, "moments.json"), "utf8"));
const all: (Moment & { log?: string })[] = [
  ...moments.run,
  ...(moments.transitions ?? []),
  ...moments.stuck,
  { name: "furthest", frame: moments.furthest.frame, at: moments.furthest.at },
  ...(moments.finish ? [{ name: "finish", frame: moments.finish.frame, at: {} }] : []),
];
const last = trace.length;
const dir = path.join(out, "views");
fs.mkdirSync(dir, { recursive: true });

function sh(cmd: string, args: string[], env: Record<string, string> = {}) {
  const r = spawnSync(cmd, args, { cwd: REACT, env: { ...process.env, RAEA_AUDIO_MUTE: "1", ...env }, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(" ")}: ${(r.stderr || r.stdout || "").split("\n").slice(-6).join(" | ")}`);
  return r.stdout;
}

// Frames are rendered in groups: one engine process per input log and overlay set, drawing every frame the views need
// from it (`render --at`). A first pass over the views plans the frames, then the groups render, then a second pass
// builds the media. A debug group renders a wrapper module beside the edit that sets the overlays, then imports it.
let planning = true;
const groups = new Map<string, { log: string; overlays: string[]; frames: Set<number> }>();
const rendered = new Map<string, string>();
function frame(log: string, overlays: string[], f: number) {
  const g = `${log}|${overlays.join(",")}`;
  if (planning) {
    if (!groups.has(g)) groups.set(g, { log, overlays, frames: new Set() });
    groups.get(g)!.frames.add(f);
    return "";
  }
  return rendered.get(`${g}|${f}`)!;
}
function renderGroups() {
  for (const [g, { log, overlays, frames: fs_ }] of groups) {
    const list = [...fs_].sort((a, b) => a - b);
    const hash = crypto.createHash("sha1").update(g).digest("hex").slice(0, 10);
    const outDir = path.join(dir, `g-${hash}`);
    fs.mkdirSync(outDir, { recursive: true });
    let entry = path.resolve(edit);
    const temp: string[] = [];
    if (overlays.length) {
      const base = path.join(path.dirname(entry), `_playtest-${process.pid}-${hash}`);
      fs.writeFileSync(base + "-on.ts", `(globalThis as any).__RAEA_OVERLAYS__ = ${JSON.stringify(overlays)};\n`);
      fs.writeFileSync(base + ".tsx", `import "./${path.basename(base)}-on.ts";\nimport "./${path.basename(entry)}";\n`);
      temp.push(base + "-on.ts", base + ".tsx");
      entry = base + ".tsx";
    }
    try {
      // at utility QoS, not -b: background QoS starves the GPU and a frame never commits in its 20 s (2026-10-18)
      const args = ["-c", "utility", BIN, "render", entry, "--input", log, "--at", list.join(","), "-o", outDir + "/"];
      // a frame can miss its 20 s commit when the machine is loaded: the group is rendered once more
      try { sh("taskpolicy", args, { RAEA_READBACK: "rgba" }); } catch { sh("taskpolicy", args, { RAEA_READBACK: "rgba" }); }
    } finally {
      for (const p of temp) fs.rmSync(p, { force: true });
    }
    list.forEach((f, i) => rendered.set(`${g}|${f}`, path.join(outDir, `frame-${String(i).padStart(5, "0")}.png`)));
    console.log(`rendered ${list.length} frames${overlays.length ? ` with ${overlays.join("+")}` : " clean"} from ${path.basename(log)}`);
  }
}
function frames(lo: number, hi: number, stride: number) {
  const out: number[] = [];
  for (let f = Math.max(1, lo); f <= Math.min(hi, last); f += stride) out.push(f);
  return out;
}

const entries: unknown[] = [];
function pass() {
for (const v of views) {
  if (only && !only.includes(v.name)) continue;
  const overlays = v.layer === "debug" ? v.overlays ?? [] : [];
  // instances: the whole run, the moments a predicate picks, or the first frame of each stretch a trace predicate holds
  let picks: (Moment & { log?: string })[];
  if (v.when === "session") picks = [{ name: "session", frame: 1, at: {} }];
  else if (typeof v.when === "function") picks = all.filter(v.when as (m: Moment) => boolean);
  else {
    const pred = v.when.frame;
    picks = trace.filter((r, i) => pred(r) && !(i > 0 && pred(trace[i - 1]))).map((r) => ({ name: v.name, frame: r.frame, at: {} }));
  }
  picks = picks.slice(0, v.limit ?? 3);
  for (const [i, m] of picks.entries()) {
    const log = path.join(out, m.log ?? "run.input.json");
    const span: [number, number] = v.when === "session" ? [1, last] : [m.frame + (v.window?.[0] ?? 0), m.frame + (v.window?.[1] ?? 0)];
    const stride = v.stride ?? 1;
    const base = path.join(dir, `${v.name}-${i}`);
    const rows = trace.filter((r) => r.frame >= span[0] && r.frame <= span[1]);
    if (planning) {
      if (v.as === "stills") [span[0], m.frame, span[1]].forEach((f) => frame(log, overlays, Math.min(Math.max(1, f), last)));
      else if (v.as === "sheet") frames(span[0], span[1], stride).slice(0, 16).forEach((f) => frame(log, overlays, f));
      else if (v.as === "clip") frames(span[0], span[1], stride).forEach((f) => frame(log, overlays, f));
      continue;
    }
    let media: string[] = [];
    if (v.as === "clip" && (span[1] - span[0]) / FPS > 60) throw new Error(`${v.name}: a ${((span[1] - span[0]) / FPS).toFixed(1)} s clip; clef takes at most 60 s`);
    if (v.as === "text") {
      const file = base + ".json";
      fs.writeFileSync(file, JSON.stringify(rows.filter((_, k) => k % stride === 0)));
      media = [file];
    } else if (v.as === "stills") {
      const fs3 = [...new Set([span[0], m.frame, span[1]].map((f) => Math.min(Math.max(1, f), last)))];
      media = fs3.map((f, k) => {
        const jpg = `${base}-${k}.jpg`;
        sh("ffmpeg", ["-v", "error", "-y", "-i", frame(log, overlays, f), "-vf", "scale=960:-2", "-q:v", "3", jpg]);
        return jpg;
      });
    } else if (v.as === "sheet") {
      const fl = frames(span[0], span[1], stride).slice(0, 16);
      const cols = Math.ceil(Math.sqrt(fl.length)), rowsN = Math.ceil(fl.length / cols);
      const list = base + ".txt";
      fs.writeFileSync(list, fl.map((f) => `file '${frame(log, overlays, f)}'`).join("\n"));
      const jpg = base + ".jpg";
      sh("ffmpeg", ["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-vf", `scale=640:-2,tile=${cols}x${rowsN}:padding=6`, "-frames:v", "1", "-q:v", "3", jpg]);
      fs.rmSync(list);
      media = [jpg];
    } else if (v.as === "clip") {
      const fl = frames(span[0], span[1], stride);
      const list = base + ".txt";
      fs.writeFileSync(list, fl.map((f) => `file '${frame(log, overlays, f)}'\nduration ${stride / FPS}`).join("\n"));
      const mp4 = base + ".mp4";
      // as clef was calibrated: x264 at 480 wide, crf 30 (apps/void/src/checks.rs media_for)
      sh("ffmpeg", ["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-vf", `scale=480:-2,fps=${FPS / stride}`, "-c:v", "libx264", "-crf", "30", "-pix_fmt", "yuv420p", mp4]);
      fs.rmSync(list);
      media = [mp4];
    } else if (v.as === "audio") {
      media = []; // not built: the window's mix alone (render's audio, cut); LEDGE has no sound
    }
    entries.push({
      view: v.name, instance: i, moment: { name: m.name, frame: m.frame, at: m.at }, frames: span, as: v.as,
      layer: v.layer ?? "clean", overlays, media: media.map((p) => path.relative(out, p)),
      facts: rows.length ? { from: rows[0], to: rows[rows.length - 1], events: rows.flatMap((r) => r.events.map((e) => ({ e, frame: r.frame }))) } : null,
    });
    console.log(`${v.name}#${i} ${v.as} ${v.layer ?? "clean"} frames ${span[0]}..${span[1]} -> ${media.map((p) => path.basename(p)).join(", ") || "(none)"}`);
  }
}
}
pass();
renderGroups();
planning = false;
pass();
// the frames and bundles were the means; the manifest names the media
for (const p of fs.readdirSync(dir)) if (p.startsWith("g-")) fs.rmSync(path.join(dir, p), { recursive: true, force: true });
fs.writeFileSync(path.join(out, "views.json"), JSON.stringify({ game: path.relative(REACT, path.resolve(edit)), run: "run.input.json", entries }, null, 1));
