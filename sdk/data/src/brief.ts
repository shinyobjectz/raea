// @raea/data/brief: a compiled brief as entities in the ECS world (the founder, 2026-10-17: the plan queryable where
// components and game systems live). Each scene, line, part and open beat is an entity with typed components, timed
// in the edit's frames from the compiled timeline (SPACE/.score/NAME.ir.json), so "which line is said at frame f" and
// "which parts show then" are queries like any other.

import { Data, bool, component, entity, read, str, u32 } from "./index";

export const Scene = component("Scene", { name: str, ord: u32 });
export const Line = component("Line", { ord: u32, scene: str, speaker: str, take: str, text: str, from: u32, to: u32 });
export const Part = component("Part", { kind: str, line: u32, band: str, from: u32, to: u32 });
export const Beat = component("Beat", { scene: str, kind: str, phrase: str, find: str, bound: bool, from: u32, to: u32 });

/**
 * The brief's graph (raea-brief parse) and its timeline (the IR scorec writes) as entities; their handles. Lines take
 * their frames from the clips cut for them; parts from the timeline's parts, each on the line whose clip it starts in;
 * open beats from the clips cut for them (a beat the footage didn't cover has no frames, and bound false).
 */
export function loadBrief(w: Data, graph: any, ir?: any) {
  const tl = ir?.timeline ?? ir ?? { clips: [], parts: [] };
  const clips: any[] = tl.clips ?? [];
  const scenes: number[] = [], lines: number[] = [], parts: number[] = [], beats: number[] = [];
  (graph.sections ?? []).forEach((s: any, i: number) => scenes.push(w.spawn(Scene.of({ name: s.name, ord: i + 1 }))));
  let ord = 0;
  const beatClips = clips.filter((c) => c.line < 0);
  let bi = 0;
  const lineOf: { ord: number; from: number; to: number }[] = [];
  for (const it of graph.items ?? []) {
    if (it.type === "line") {
      ord++;
      const cs = clips.filter((c) => c.line === ord - 1);
      const from = cs.length ? Math.min(...cs.map((c) => c.start)) : 0, to = cs.length ? Math.max(...cs.map((c) => c.end)) : 0;
      lineOf.push({ ord, from, to });
      lines.push(w.spawn(Line.of({ ord, scene: it.section ?? "", speaker: it.speaker ?? "", take: it.take ?? "", text: it.text, from, to })));
    } else if (it.type === "beat") {
      // a beat's clip comes in the timeline where the beat stands; the graph doesn't say which were bound, the clips do
      const c = bi < beatClips.length ? beatClips[bi] : null;
      const bound = !!c && (lineOf.length ? c.start >= lineOf[lineOf.length - 1].to : true);
      if (bound) bi++;
      beats.push(w.spawn(Beat.of({ scene: it.section ?? "", kind: it.kind, phrase: it.phrase, find: (it.find ?? []).map((f: any) => typeof f === "string" ? f : f.picture).join(" "),
        bound, from: bound ? c.start : 0, to: bound ? c.end : 0 })));
    }
  }
  for (const p of tl.parts ?? []) {
    const on = lineOf.find((l) => l.from <= p.start && p.start < l.to);
    parts.push(w.spawn(Part.of({ kind: p.component ?? p.kind, line: on?.ord ?? 0, band: p.band ?? "", from: p.start, to: p.end })));
  }
  return { scenes, lines, parts, beats };
}

/** The line said at frame f, if any. */
export function lineAt(w: Data, f: number) {
  for (const [e, l] of w.query(entity, read(Line))) if (l.from <= f && f < l.to) return { entity: e, ...l };
  return null;
}

/** The parts showing at frame f. */
export function partsAt(w: Data, f: number) {
  const out: { entity: number; kind: string; line: number; band: string; from: number; to: number }[] = [];
  for (const [e, p] of w.query(entity, read(Part))) if (p.from <= f && f < p.to) out.push({ entity: e, ...p });
  return out;
}
