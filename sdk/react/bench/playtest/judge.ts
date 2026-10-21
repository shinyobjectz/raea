// A playtest's checks on a run (tier iii, sdk/react/docs/playtest.md): the rules from the run's report and trace (no model),
// then clef-omni once per view instance for the checks that name that view, each sent as the native clef question the
// DSL built (sdk/motion/src/playtest.ts) and read with its read(). Synthetic or committed fixtures only: creators' footage
// never goes to a cloud model. Writes OUT/judge.json; prints each finding and the calls and cost.
//   bun sdk/react/bench/playtest/judge.ts OUT [--playtest EDIT.playtest.ts] [--dry]
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { loadModule } from "./load";
import { read, type Check, type ClefQuestion, type PlaytestSpec } from "../../../motion/src/playtest";


// ---- rules: what the trace answers -----------------------------------------------------------------------------
/** The machine rules (sdk/motion/src/playtest.ts coversStates ... reaches): report.json's `machines`, for one machine or all. */
export function machineRule(report: any, name: string, args: any): { faults: number; says: string } | null {
  const all: Record<string, any> = report.machines ?? {};
  const ids = args?.machine ? [args.machine] : Object.keys(all);
  if (!["state_coverage", "transition_coverage", "machine_softlock", "unreachable_states", "reached_state"].includes(name)) return null;
  if (!ids.length) return { faults: 1, says: report.machinesNote ?? "the run has no state machines (the game's playtest export has no `machines`)" };
  const missing = ids.filter((id) => !all[id]);
  if (missing.length) return { faults: 1, says: `no machine ${missing.join(", ")} in the run (machines: ${Object.keys(all).join(", ") || "none"})` };
  const says: string[] = [];
  let faults = 0;
  for (const id of ids) {
    const m = all[id];
    if (name === "state_coverage" || name === "transition_coverage") {
      const c = name === "state_coverage" ? m.stateCoverage : m.transitionCoverage;
      const got = name === "state_coverage" ? c.reached : c.fired;
      const share = c.of ? got / c.of : 1;
      const short = share + 1e-9 < (args?.min ?? 1);
      if (short) faults += c.missing.length;
      says.push(`${id}: ${got} of ${c.of} ${name === "state_coverage" ? "states reached" : "transitions fired"} (${Math.round(share * 100)}%)${c.missing.length ? `; never: ${c.missing.join(", ")}` : ""}`);
    } else if (name === "machine_softlock") {
      const loops = args?.loops ? m.loops : [];
      faults += m.softlocks.length + loops.length;
      says.push(`${id}: ${m.softlocks.length ? m.softlocks.map((x: any) => `${x.state} (${x.why}${x.longestStay ? `, stayed ${x.longestStay.frames} frames through ${x.longestStay.changes} input changes` : ""})`).join(", ") : "no softlocks"}`
        + (loops.length ? `; loops: ${loops.map((l: any) => `${l.entity} ${l.edge} x${l.times} at frames ${l.frames.join("-")}`).join(", ")}` : "")
        + (m.unconfirmed.length ? `; never left but unconfirmed: ${m.unconfirmed.map((x: any) => x.state).join(", ")}` : ""));
    } else if (name === "unreachable_states") {
      faults += m.unreachable.length;
      says.push(`${id}: ${m.unreachable.length ? `unreachable: ${m.unreachable.join(", ")}` : "every state reachable"}`);
    } else {
      const n = m.reached[args.state] ?? 0;
      if (!n) faults++;
      says.push(`${id}: ${args.state} ${n ? `entered ${n} times` : "never entered"}`);
    }
  }
  return { faults, says: says.join("; ") };
}

function rule(report: any, name: string, args?: any): { faults: number; says: string } {
  const m = machineRule(report, name, args);
  if (m) return m;
  switch (name) {
    case "finished":
      return report.finished ? { faults: 0, says: `finished in ${report.framesToFinish} frames (${report.secondsToFinish} s) by ${report.finishedBy}` }
        : { faults: 1, says: `no bot reached the goal; play stops at progress ${report.ceiling.progress} (${JSON.stringify(report.ceiling.at)})` };
    case "softlocks":
      return { faults: report.softlocks.found, says: `${report.softlocks.found} of ${report.softlocks.probed} probed states couldn't get back to the frontier` };
    case "has_outcome":
      return { faults: report.finished ? 0 : 1, says: report.finished ? "the run ends in a win" : "nothing in the run ended the game: no win reached, no game over" };
    case "death_loop": {
      const total = report.deathSpots.reduce((a: number, d: { deaths: number }) => a + d.deaths, 0);
      const top2 = report.deathSpots.slice(0, 2).reduce((a: number, d: { deaths: number }) => a + d.deaths, 0);
      const share = total ? top2 / total : 0;
      return { faults: !report.finished && share > 0.6 ? 1 : 0, says: `${Math.round(share * 100)}% of ${total} search deaths in the top two 200-px spots (${JSON.stringify(report.deathSpots.slice(0, 2))})` };
    }
  }
  return { faults: 0, says: `unknown rule ${name}` };
}

// ---- the paragraph clef reads before the media -----------------------------------------------------------------
function context(keys: string[], entry: any): string {
  const parts: string[] = [];
  if (keys.includes("session")) parts.push("A small 2D side-scrolling platformer. The frames come from one automated play of it, in order.");
  if (keys.includes("moment") && entry?.moment) {
    const m = entry.moment;
    parts.push(`This is the moment "${m.name}" of an automated play, at frame ${m.frame} (120 frames a second); the clip runs from frame ${entry.frames[0]} to ${entry.frames[1]}.`);
  }
  if (keys.includes("observe") && entry?.facts?.from) {
    const f = entry.facts.from;
    parts.push(`At the clip's start the player is ${f.onGround ? "on the ground" : "in the air"}, ${f.groundEndsIn} px before the ground ends${f.gapWidth ? `, with a ${f.gapWidth} px gap after it` : ""}.`);
  }
  return parts.join(" ");
}

function cloudflare() {
  // the environment first, then raea's settings file ($RAEA_ENV_FILE, else ~/.raea/env)
  let file = "";
  try {
    file = fs.readFileSync(process.env.RAEA_ENV_FILE ?? path.join(os.homedir(), ".raea/env"), "utf8");
  } catch {}
  const get = (k: string) => process.env[k] || file.split("\n").find((l) => l.startsWith(k + "="))?.slice(k.length + 1).trim().replace(/^"|"$/g, "");
  const acct = get("CLOUDFLARE_ACCOUNT_ID"), token = get("CLOUDFLARE_API_TOKEN");
  if (!acct || !token) throw new Error("set CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN (environment or ~/.raea/env)");
  return { acct, token };
}

/** One clef-omni call: native questions by id, about one media file (an image, a video or an audio clip) after `state`. */
export async function ask(media: string, state: string, questions: Record<string, ClefQuestion>): Promise<{ answers: Record<string, any>; cost: number }> {
  const { acct, token } = cloudflare();
  const body: Record<string, unknown> = { model: "clef-omni", state, questions };
  if (media) {
    const b64 = fs.readFileSync(media).toString("base64");
    const ext = path.extname(media).slice(1);
    if (["mp4", "webm", "mov"].includes(ext)) body.videos = [`data:video/${ext === "mov" ? "quicktime" : ext};base64,${b64}`];
    else if (["wav", "mp3", "m4a"].includes(ext)) body.audio = [`data:audio/${ext === "mp3" ? "mpeg" : ext};base64,${b64}`];
    else body.images = [`data:image/${ext === "png" ? "png" : ext === "webp" ? "webp" : "jpeg"};base64,${b64}`];
  }
  const res = await fetch(`https://api.cloudflare.com/client/v4/accounts/${acct}/ai/run/@cf/cloudflare/clef-omni`, {
    method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify(body),
    signal: AbortSignal.timeout(120_000),
  });
  const j: any = await res.json();
  if (!j.success) throw new Error(`clef: ${JSON.stringify(j.errors)}`);
  return { answers: j.result.answers, cost: (j.result?.usage?.input_tokens ?? 0) * 0.15 / 1e6 };
}

if (import.meta.main) {
  const out = process.argv[2];
  const dry = process.argv.includes("--dry");
  const report = JSON.parse(fs.readFileSync(path.join(out, "report.json"), "utf8"));
  // the rules need only the report: without rendered views (views.ts not run) the clef checks say there's nothing to see
  const viewsAt = path.join(out, "views.json");
  const manifest = fs.existsSync(viewsAt) ? JSON.parse(fs.readFileSync(viewsAt, "utf8")) : { game: report.game.replace(/^react\//, ""), entries: [] };
  const ptAt = process.argv.indexOf("--playtest");
  const ptFile = ptAt > 0 ? process.argv[ptAt + 1] : path.resolve(import.meta.dir, "../..", manifest.game.replace(/\.tsx$/, ".playtest.ts"));
  const pack: PlaytestSpec = (await loadModule(ptFile)).default;
  const findings: any[] = [];
  for (const c of pack.checks) {
    if (c.kind !== "rule") continue;
    const r = rule(report, c.rule, c.args);
    findings.push({ id: c.id, scope: c.common.scope, eye: "rule", status: c.common.status ?? "advisory", value: r.faults, fault: r.faults > 0, says: r.says, fix: r.faults ? c.common.fix : "" });
  }
  // clef checks grouped by the view instance they look at (the first instance of the view each names), one call each
  type Eye = Exclude<Check, { kind: "rule" }>;
  const byView = new Map<string, Eye[]>();
  for (const c of pack.checks) if (c.kind !== "rule") byView.set(c.view.name, [...(byView.get(c.view.name) ?? []), c]);
  let calls = 0, cost = 0;
  for (const [v, checks] of byView) {
    const entry = manifest.entries.find((e: any) => e.view === v && e.media.length);
    if (!entry) {
      for (const c of checks) findings.push({ id: c.id, scope: c.common.scope, eye: "clef", status: "advisory", value: null, fault: false, says: `no ${v} view in this run (nothing to look at)`, fix: "" });
      continue;
    }
    const keys = [...new Set(checks.flatMap((c) => c.common.context ?? []))];
    const state = context(keys, entry);
    const questions = Object.fromEntries(checks.map((c) => [c.id, c.clef]));
    if (dry) { console.log(`would ask clef about ${v} (${entry.media[0]}): ${JSON.stringify(questions)}\n  state: ${state}`); continue; }
    const r = await ask(path.join(out, entry.media[0]), state, questions);
    calls++;
    cost += r.cost;
    for (const c of checks) {
      const got = read(c, r.answers[c.id]);
      findings.push({ id: c.id, scope: c.common.scope, eye: "clef", type: c.clef.type, view: v, layer: c.view.layer, media: entry.media[0], status: c.common.status ?? "advisory",
        value: +got.value.toFixed(3), fault: got.fault, says: got.says, fix: got.fault ? c.common.fix : "" });
    }
  }
  const verdict = { pack: `${pack.pack}@${pack.version}`, game: manifest.game, findings, calls, cost: +cost.toFixed(5) };
  fs.writeFileSync(path.join(out, "judge.json"), JSON.stringify(verdict, null, 1));
  for (const f of findings) console.log(`${f.fault ? (f.status === "warn" ? "WARN" : "advisory fault") : "ok"}  ${f.id}  ${f.says}`);
  console.log(`clef calls ${calls}, cost $${cost.toFixed(5)}`);
}
