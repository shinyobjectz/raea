// raea/check: a check is a system over the world (docs/direction.feature DR4). `raea check` reads each composition's
// frames into worlds holding the components `raea world` prints (raea-core-facts; their types are facts.gen.ts,
// generated from the Rust), joins the composition's newest render, and runs every check that applies on the frames it
// says it ranges over. A check finds entities, measures, and calls `found`; it never prints. raea's own checks are
// the same thing written in Rust, run under the same names, and a check reads the same for a mesh, a caption, a cut
// or a cue: nothing here is 3D's.
//
//   // checks/clear.ts, run by a line in the project's taste.ts:
//   //   { text: "Captions clear the button band", check: "checks/clear.ts", values: { band: 0.8 } }
//   import { check, found, type Text } from "raea/check";
//   export default check({ id: "caption-clear", text: "Captions clear the button band" }, (world, frame, values) => {
//     for (const t of world.find({ with: ["Text", "Layer"] })) {
//       const box = world.component<Text>(t, "Text")?.box;
//       if (box && box[3] > Number(values.band ?? 0.8)) found({ about: [t], says: "the caption runs into the button band" });
//     }
//   });

import { createWorld, type World } from "@raea/app/ecs";
import type { About, Finding, Level, Measure, Part } from "./facts.gen";

export type * from "./facts.gen";
export type { World };

/** The frames a check runs on: every frame; the first frame of each shot after the first, and the frame before it;
 * every frame a line is said or sung on; once, on the first frame (for what is the piece's, as its mix); or these. */
export type Frames = "all" | "cuts" | "lines" | "once" | number[];

export interface CheckMeta {
  /** what `raea check --only` names, and its findings' check */
  id: string;
  /** the line it holds the work to */
  text: string;
  /** "render": it reads what only a render has (Contrast, Picture, Mix, Onset, Covers); with none, it waits for one */
  needs?: "render";
  /** how bad what it finds is, where a finding doesn't say (error when unset) */
  level?: Level;
  /** where it runs ("all" when unset) */
  frames?: Frames;
  /** what it reads: a composition's frames (the default), a story with no frames (`raea check NAME.story.ts` runs only
   *  these and "both", with its `after` alone), or both (docs/story.feature ST5a) */
  over?: "frames" | "story" | "both";
}

/** A rule's values for its check (a taste line's `values`, an ask's). */
export type Values = Record<string, number | string | boolean>;
/** What one rule's run of a check keeps from frame to frame: empty on its first frame, its own (two asks that run
 * one check each have theirs). */
export type Memo = Record<string, any>;
/** The system: a frame's world, the frame, the rule's values, and what this run has kept. */
export type CheckFn = (world: World, frame: number, values: Values, memo: Memo) => void;
/** Run once after the last frame, with what the frames kept: for what only the whole run can say (how long a shot
 * held, that no frame had the part an ask is about). A finding here has no frames unless it gives them. */
export type AfterFn = (values: Values, memo: Memo) => void;
export interface Check extends CheckMeta {
  /** its system, run on each frame it ranges over */
  run: CheckFn;
  /** run once past its last frame */
  after?: AfterFn;
}

/** A check: what it holds the work to, the system that finds where the work doesn't hold, and what it says once the
 * frames are run. */
export function check(meta: CheckMeta, run: CheckFn, after?: AfterFn): Check {
  return { ...meta, run, after };
}

/** The names an entity goes by, as `named` matches them: its part's name, its model file's name (no folder, no
 * extension), its shot's name and its layer key; and, each only by its prefix, the component that drew it
 * (`component:Orb`) and its model file in full (`file:props/orb.glb`). */
function names(world: World, id: number): { bare: string[]; component?: string; file?: string } {
  const c = world.components(id) as Record<string, Record<string, unknown> | undefined>;
  const str = (v: unknown) => (typeof v === "string" && v ? v : undefined);
  const file = str(c.Part?.src);
  const stem = file?.split("/").pop()?.replace(/\.[^.]+$/, "");
  return { bare: [str(c.Part?.name), stem, str(c.Shot?.name), str(c.Source?.key)].filter((v): v is string => !!v), component: str(c.Source?.component), file };
}

/** Whether entity `id` is one of `kinds` (a list, or one string of them split at commas): a bare kind is a part's,
 * shot's or layer's name or a model file's, as written or as its start ("desk" is desk-3 and Desk.glb), any case;
 * `component:Orb` is what the Orb component drew, and `file:props/orb.glb` what that model file is. Until a
 * component's taxonomy (DR11) says what a thing is, an ask's check names it this way. */
export function named(world: World, id: number, kinds: string | string[] | undefined): boolean {
  const ks = (Array.isArray(kinds) ? kinds : String(kinds ?? "").split(",")).map((k) => k.trim().toLowerCase()).filter(Boolean);
  if (!ks.length) return false;
  const n = names(world, id);
  const starts = (v: string | undefined, k: string) => !!v && v.toLowerCase().startsWith(k);
  return ks.some((k) => {
    if (k.startsWith("component:")) return n.component?.toLowerCase() === k.slice(10);
    if (k.startsWith("file:")) return !!n.file && n.file.toLowerCase().endsWith(k.slice(5));
    return n.bare.some((v) => starts(v, k));
  });
}

/** `v` turned by a rotation `q` (x, y, z, w), as a part's `rotation` turns its own space into the world's. */
export function turned(q: number[], v: number[]): number[] {
  const [x, y, z, w] = q;
  const t = [2 * (y * v[2] - z * v[1]), 2 * (z * v[0] - x * v[2]), 2 * (x * v[1] - y * v[0])];
  return [v[0] + w * t[0] + (y * t[2] - z * t[1]), v[1] + w * t[1] + (z * t[0] - x * t[2]), v[2] + w * t[2] + (x * t[1] - y * t[0])];
}

/** A part as a box turned as it is: its centre, its axes and its half sizes along them, in world metres. A part with
 * no box of its own is its world box; one with neither has none. */
function boxOf(p: Part): { c: number[]; axes: number[][]; half: number[] } | null {
  const unit = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  if (!p.own) return p.min && p.max ? { c: [0, 1, 2].map((k) => (p.min![k] + p.max![k]) / 2), axes: unit, half: [0, 1, 2].map((k) => (p.max![k] - p.min![k]) / 2) } : null;
  const [lo, hi] = p.own;
  const c = turned(p.rotation, [0, 1, 2].map((k) => ((lo[k] + hi[k]) / 2) * p.scale[k])).map((v, k) => v + p.at[k]);
  return { c, axes: unit.map((u) => turned(p.rotation, u)), half: [0, 1, 2].map((k) => (Math.abs(hi[k] - lo[k]) / 2) * Math.abs(p.scale[k])) };
}

/** How far two parts are in each other, in metres: the least overlap of their own boxes, turned as they are, along
 * the axes that could part them (0: apart, or a part with no box). */
export function inside(a: Part, b: Part): number {
  const p = boxOf(a);
  const q = boxOf(b);
  if (!p || !q) return 0;
  const dot = (u: number[], v: number[]) => u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
  const cross = (u: number[], v: number[]) => [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
  const d = [0, 1, 2].map((k) => q.c[k] - p.c[k]);
  let least = Infinity;
  for (const ax of [...p.axes, ...q.axes, ...p.axes.flatMap((u) => q.axes.map((v) => cross(u, v)))]) {
    const len = Math.hypot(ax[0], ax[1], ax[2]);
    if (len < 1e-9) continue;
    const n = ax.map((v) => v / len);
    const reach = (o: NonNullable<typeof p>) => o.half.reduce((s, h, k) => s + h * Math.abs(dot(o.axes[k], n)), 0);
    const overlap = reach(p) + reach(q) - Math.abs(dot(d, n));
    if (overlap <= 0) return 0;
    least = Math.min(least, overlap);
  }
  return least === Infinity ? 0 : least;
}

/** What a check found: the entities it is about, what it says, and, where they aren't the check's or the frame's,
 * its level and frames (its first, and the frame after its last); the number it turns on and the fix, when there
 * are. */
export interface Found {
  /** the entities it is about, by their ids in this frame's world, or by name (a check's `after` has no world) */
  about?: (number | string)[];
  /** what is wrong, in a line a person reads */
  says: string;
  /** its level, where it isn't its rule's or its check's */
  level?: Level;
  /** its first frame and the frame after its last (this frame alone when unset) */
  frames?: [number, number];
  /** the number it turns on: its name, value and unit, and the limit it broke */
  measure?: Omit<Measure, "limit"> & { limit?: number };
  /** what to change, when there is one thing to change */
  fix?: string;
}

let running: { world: World; frame: number; check: Check; plan: Planned; out: Finding[] } | null = null;

/** Says what the running check found. Only inside a check's system, as raea check runs it. */
export function found(f: Found): void {
  if (!running) throw new Error("found() is for a check's system: raea check runs it on each frame");
  const { world, frame, check: c, plan, out } = running;
  out.push({
    check: c.id,
    // the finding's own level, else its rule's, else the check's
    level: f.level ?? plan.level ?? c.level ?? "error",
    about: (f.about ?? []).map((id) => (typeof id === "string" ? ({ name: id } as About) : aboutOf(world, id))),
    // past the last frame (a check's `after`), none unless it says
    frames: f.frames ?? (frame < 0 ? null : [frame, frame + 1]),
    says: f.says,
    measure: f.measure ? { ...f.measure, limit: f.measure.limit ?? null } : null,
    fix: f.fix ?? null,
    by: plan.by ?? "",
  } as Finding);
}

/** An entity as a finding names it: its words, its part's or shot's name, its sound, its component; its layer key. */
function aboutOf(world: World, id: number): About {
  const c = world.components(id) as Record<string, Record<string, unknown> | undefined>;
  const name = [c.Text?.words, c.Part?.name, c.Shot?.name, c.Cue?.source, c.Clip?.source, c.Source?.component].find((v) => typeof v === "string" && v);
  const key = c.Source?.key;
  return { name: String(name ?? `entity ${id}`), ...(typeof key === "string" ? { key } : {}) } as About;
}

/** A frame's facts as `raea world --json` prints them. */
export interface Facts {
  /** the frame, from the composition's first */
  frame: number;
  /** the composition's width in pixels */
  width: number;
  /** the composition's height in pixels */
  height: number;
  /** the composition's frames a second */
  fps: number;
  /** each entity: its id, its parent's, and its components by name */
  entities: { id: number; parent?: number | null; components: Record<string, unknown> }[];
  /** what is the frame's or the piece's, not an entity's (Rendered, Picture, Mix, Waits) */
  resources?: Record<string, unknown>;
}

/** A frame's facts as a raea/ecs world: the same components, the frame's size and rate as the `Frame` resource, and
 * what the render added as resources. An entity's references to others (a Covered's `layer`, a relation's `to`)
 * are its id in this world. */
export function worldOf(facts: Facts): World {
  const w = createWorld({ native: false });
  const ids = new Map<number, number>();
  for (const e of facts.entities) ids.set(e.id, w.add({}, e.parent != null && ids.has(e.parent) ? { parent: ids.get(e.parent) } : {}));
  const to = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(to);
    if (!v || typeof v !== "object") return v;
    const o = { ...(v as Record<string, unknown>) };
    for (const k of ["layer", "to"]) if (typeof o[k] === "number" && ids.has(o[k] as number)) o[k] = ids.get(o[k] as number);
    return o;
  };
  for (const e of facts.entities) w.insert(ids.get(e.id)!, Object.fromEntries(Object.entries(e.components).map(([k, v]) => [k, to(v)])));
  w.resource("Frame", { frame: facts.frame, width: facts.width, height: facts.height, fps: facts.fps });
  for (const [k, v] of Object.entries(facts.resources ?? {})) w.resource(k, v);
  return w;
}

/** Each check's declaration, as raea check reads it before it reads any frame. */
export function declared(checks: Check[]): CheckMeta[] {
  return checks.map((c, i) => {
    if (!c || typeof c.run !== "function") throw new Error(`check ${i + 1}: its file's default export isn't a check({ id, text }, system)`);
    return { id: c.id, text: c.text, needs: c.needs, level: c.level ?? "error", frames: c.frames ?? "all" };
  });
}

/** Which check runs on which frames with which values, as raea check plans it from a rule: the rule's level (none:
 * the check's own) and where the rule was written. */
export interface Planned {
  /** the check, by its place in the list run */
  check: number;
  /** the frames it runs on */
  frames: number[];
  /** the rule's values, which the check's system is given */
  values: Values;
  /** the rule's level, which its findings take where they don't say their own */
  level?: Level | null;
  /** where the rule was written, which its findings carry */
  by?: string;
}

/** Every planned check run on its frames' worlds: what they found, a run of frames finding the same thing as one
 * finding over those frames, and each check that threw, with what it threw. */
export function runChecks(checks: Check[], plan: Planned[], worlds: Facts[]): { findings: Finding[]; threw: { check: string; says: string }[] } {
  const out: Finding[] = [];
  const threw: { check: string; says: string }[] = [];
  // each planned run's memo, kept across its frames
  const memos: Memo[] = plan.map(() => ({}));
  const call = (c: Check, at: string, f: () => void) => {
    try {
      f();
    } catch (e) {
      if (!threw.some((t) => t.check === c.id)) threw.push({ check: c.id, says: `${at}: ${e instanceof Error ? e.message : String(e)}` });
    } finally {
      running = null;
    }
  };
  for (const facts of worlds) {
    if (!plan.some((p) => p.frames.includes(facts.frame))) continue;
    const world = worldOf(facts);
    plan.forEach((p, i) => {
      if (!p.frames.includes(facts.frame)) return;
      const c = checks[p.check];
      running = { world, frame: facts.frame, check: c, plan: p, out };
      call(c, `at frame ${facts.frame}`, () => c.run(world, facts.frame, p.values, memos[i]));
    });
  }
  // what only the whole run can say, once, with no frame's world
  const none = createWorld({ native: false });
  plan.forEach((p, i) => {
    const c = checks[p.check];
    if (!c.after || threw.some((t) => t.check === c.id)) return;
    running = { world: none, frame: -1, check: c, plan: p, out };
    call(c, "after its frames", () => c.after!(p.values, memos[i]));
  });
  return { findings: joined(out), threw };
}

/** Findings that say the same about the same on frames one after another, as one over all of them. */
function joined(fs: Finding[]): Finding[] {
  const out: Finding[] = [];
  const last = new Map<string, Finding>();
  for (const f of fs) {
    const k = JSON.stringify([f.check, f.level, f.says, f.about, f.measure, f.fix, f.by]);
    const prev = last.get(k);
    const [a, b] = (f.frames ?? [0, 0]) as [number, number];
    const pf = prev?.frames as [number, number] | undefined;
    // a run's end is the frame after its last, so the next frame starts where it ends
    if (prev && pf && a <= pf[1]) {
      prev.frames = [pf[0], Math.max(pf[1], b)] as Finding["frames"];
      continue;
    }
    out.push(f);
    last.set(k, f);
  }
  return out;
}
