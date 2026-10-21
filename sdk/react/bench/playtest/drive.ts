// Tier (ii) spike (sdk/react/docs/playtest.md): clef-omni plays. The game's EDIT.playtest.ts declares the decision (the DSL's
// decide(): a clef `choice` over named actions). Each decision, the engine renders the frame the run has reached (from
// the input so far), clef reads it with the game's observation as its state paragraph and answers that question; the
// most likely action is held for the decision's hold, stepped on the pure fold. No free-form JSON: the answer is a
// distribution over a closed menu, so it can't be malformed.
//   bun sdk/react/bench/playtest/drive.ts EDIT.tsx OUT [--decisions 36] [--hold 30] [--text-only]
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { loadGame, loadModule } from "./load";
import { ask as askClef } from "./judge";
import type { PlaytestSpec } from "../../../motion/src/playtest";

const [edit, out] = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const arg = (k: string, d: number) => { const i = process.argv.indexOf(k); return i > 0 ? Number(process.argv[i + 1]) : d; };
const spec: PlaytestSpec = (await loadModule(edit.replace(/\.tsx$/, ".playtest.ts"))).default;
const decision = spec.decisions?.[0];
if (!decision) throw new Error(`${edit}: its playtest declares no decision`);
const DECISIONS = arg("--decisions", 36), HOLD = arg("--hold", decision.hold);
const textOnly = process.argv.includes("--text-only") || decision.sees === "state";
const REACT = path.resolve(import.meta.dir, "../..");
fs.mkdirSync(out, { recursive: true });
const { playtest: p } = await loadGame(edit);
const menu = Object.keys(decision.actions);
const log: Record<string, { keys: string[] }> = {};
let s = p.initial, f = 0, calls = 0, cost = 0, deaths = 0;
const steps: unknown[] = [];
const t0 = performance.now();
for (let d = 0; d < DECISIONS && p.outcome?.(s, f) !== "won"; d++) {
  const obs = p.observe ? p.observe(s, f) : {};
  let media = "";
  if (!textOnly) {
    const logFile = path.join(out, "drive.input.json"), png = path.join(out, "now.png");
    fs.writeFileSync(logFile, JSON.stringify(log));
    // at utility QoS (background QoS starves the GPU: no frame commits); a render that fails under load is tried again
    const render = () => spawnSync("taskpolicy", ["-c", "utility", path.join(REACT, "target/release/raea-motion"), "render", path.resolve(edit), "--input", logFile, "--frame", String(f), "-o", png],
      { cwd: REACT, env: { ...process.env, RAEA_READBACK: "rgba", RAEA_AUDIO_MUTE: "1" }, encoding: "utf8" });
    let r = render();
    if (r.status !== 0) r = render();
    if (r.status !== 0) throw new Error(`render (exit ${r.status}, signal ${r.signal}): ${(r.stderr + r.stdout).split("\n").filter((l) => !l.includes("WARN")).slice(-4).join(" | ")}`);
    media = path.join(out, "now.jpg");
    spawnSync("ffmpeg", ["-v", "error", "-y", "-i", png, "-vf", "scale=640:-2", "-q:v", "4", media]);
  }
  const state = `The game's state as JSON (pixels in the level's units; x grows to the right, y grows downward): ${JSON.stringify(obs)}`;
  const r = await askClef(media, state, { [decision.id]: decision.clef });
  calls++;
  cost += r.cost;
  const probs = r.answers[decision.id]?.probabilities ?? {};
  const pick = Math.max(0, menu.indexOf(r.answers[decision.id]?.choice));
  const keys = decision.actions[menu[pick]];
  for (let i = 0; i < HOLD; i++) {
    log[String(f)] = { keys: [...keys] };
    const after = p.step(s, { keys: [...keys] } as any, f);
    if (p.events?.died?.(s, after)) deaths++;
    s = after;
    f++;
  }
  steps.push({ decision: d, frame: f - HOLD, x: obs.x, move: menu[pick], probs, progress: +(p.progress?.(s, f) ?? 0).toFixed(4) });
  console.log(`${d} f${f - HOLD} x=${obs.x} -> ${menu[pick]} ${JSON.stringify(probs)}`);
}
const result = {
  game: path.relative(REACT, path.resolve(edit)), mode: textOnly ? "text" : "image+text", decisions: steps.length, hold: HOLD, frames: f,
  progress: +(p.progress?.(s, f) ?? 0).toFixed(4), won: p.outcome?.(s, f) === "won", deaths, calls, cost: +cost.toFixed(5),
  seconds: Math.round((performance.now() - t0) / 1000), steps,
};
fs.writeFileSync(path.join(out, "drive.json"), JSON.stringify(result, null, 1));
fs.writeFileSync(path.join(out, "drive.input.json"), JSON.stringify(log));
for (const x of ["now.png", "now.jpg"]) fs.rmSync(path.join(out, x), { force: true });
console.log(JSON.stringify({ ...result, steps: undefined }));
