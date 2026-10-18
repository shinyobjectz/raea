// @raea/data/facts: what a brief is checked against, as tables: its compiled graph (raea-brief parse), the edit's
// words (NAME.words.json), perception's moments (SPACE/.score/moments.jsonl, score/compiler/moments.py), the face on
// each frame the compiler wrote (NAME.perception.json, frame-keyed), and what each frame of the render shows
// (NAME.render.json, raea-motion render --manifest). A check is then a query over them; textOverFace is the first.

import { Table, bool, f32, f64, join, str, table, u32, u8 } from "./index";

export const LineS = { scene: str, ord: u32, speaker: str, take: str, text: str, delivery: str, punch: f32, parts: str };
export const PartS = { scene: str, line: u32, kind: str, anchor: str, place: str };
export const BeatS = { scene: str, kind: str, phrase: str, find: str };
export const CheckS = { rule: str, form: str };
export const WordS = { from: u32, to: u32, text: str };
export const MomentS = {
  take: str, file: str, from: f64, to: f64, faces: u8, speaker: bool, speaking: bool, eye: bool, framing: str,
  motion: str, text: bool, screen: bool, watermark: bool, luma: f32, said: bool, words: str,
};
export const BoxS = { frame: u32, x: f32, y: f32, w: f32, h: f32 };
export const FrameTextS = { frame: u32, text: str, x: f32, y: f32, w: f32, h: f32, opacity: f32 };
export const FrameVideoS = { frame: u32, src: str, at: f64, x: f32, y: f32, w: f32, h: f32, opacity: f32 };

/** A brief's graph (raea-brief parse) as tables of its lines, parts, open beats and checks. */
export function graphTables(g: any) {
  const lines = table(LineS, [], "line"), parts = table(PartS, [], "part"), beats = table(BeatS, [], "beat"), checks = table(CheckS, [], "check");
  let ord = 0;
  for (const it of g.items ?? []) {
    if (it.type === "line") {
      ord++;
      lines.push({ scene: it.section ?? "", ord, speaker: it.speaker ?? "", take: it.take ?? "", text: it.text, delivery: it.delivery ?? "",
        punch: typeof it.punch === "number" ? it.punch : 0, parts: (it.notes ?? []).filter((n: any) => n.kind).map((n: any) => n.kind).join(",") });
      for (const n of it.notes ?? []) {
        if (!n.kind) continue;
        const place = (n.utils ?? []).find((u: string) => u.startsWith("place-"))?.slice(6) ?? "";
        parts.push({ scene: it.section ?? "", line: ord, kind: n.kind, anchor: n.anchor ?? "", place });
      }
    } else if (it.type === "beat") {
      beats.push({ scene: it.section ?? "", kind: it.kind, phrase: it.phrase, find: (it.find ?? []).map((f: any) => typeof f === "string" ? f : f.picture).join(" ") });
    }
  }
  for (const c of g.checks ?? []) checks.push({ rule: c.rule, form: c.form });
  return { lines, parts, beats, checks };
}

/** NAME.words.json: [{text, from, to}] in the edit's frames. */
export function wordsTable(words: { text: string; from: number; to: number }[]): Table<typeof WordS> {
  return table(WordS, words.map((w) => ({ from: w.from, to: w.to, text: w.text })), "word");
}

/** SPACE/.score/moments.jsonl: one moment a line. */
export function momentsTable(jsonl: string): Table<typeof MomentS> {
  const t = table(MomentS, [], "moment");
  for (const l of jsonl.split("\n")) {
    if (!l.trim()) continue;
    const m = JSON.parse(l);
    t.push({ take: m.take, file: m.file, from: m.from, to: m.to, faces: m.faces ?? 0, speaker: !!m.speaker, speaking: !!m.speaking, eye: !!m.eye,
      framing: m.framing ?? "", motion: m.motion ?? "", text: !!m.text, screen: !!m.screen, watermark: !!m.watermark, luma: m.luma ?? 0, said: !!m.said, words: m.words ?? "" });
  }
  return t;
}

/** NAME.perception.json: {frame: {safe, face?}}, the face on each frame from that frame until the next key. */
export function facesTable(perception: Record<string, { face?: { x: number; y: number; w: number; h: number } }>, frames: number): Table<typeof BoxS> {
  const keys = Object.keys(perception).map(Number).sort((a, b) => a - b);
  const t = table(BoxS, [], "face");
  for (let k = 0; k < keys.length; k++) {
    const face = perception[String(keys[k])].face;
    if (!face) continue;
    const end = k + 1 < keys.length ? keys[k + 1] : frames;
    for (let f = keys[k]; f < end; f++) t.push({ frame: f, x: face.x, y: face.y, w: face.w, h: face.h });
  }
  return t;
}

/** NAME.render.json: {fps, width, height, frames: [{frame, videos: [{src, at, box, opacity}], text: [{text, box, opacity}]}]} (or the frames alone). */
export function renderTables(manifest: any) {
  const text = table(FrameTextS, [], "frame_text"), videos = table(FrameVideoS, [], "frame_video");
  for (const fr of Array.isArray(manifest) ? manifest : manifest.frames ?? []) {
    for (const t of fr.text ?? []) text.push({ frame: fr.frame, text: t.text, x: t.box[0], y: t.box[1], w: t.box[2], h: t.box[3], opacity: t.opacity ?? 1 });
    for (const v of fr.videos ?? []) videos.push({ frame: fr.frame, src: v.src, at: v.at ?? 0, x: v.box[0], y: v.box[1], w: v.box[2], h: v.box[3], opacity: v.opacity ?? 1 });
  }
  return { text, videos };
}

/** The share of box b's area that box a covers. */
export function covers(a: { x: number; y: number; w: number; h: number }, b: { x: number; y: number; w: number; h: number }): number {
  const ix = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
  const iy = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  return b.w * b.h > 0 ? (ix * iy) / (b.w * b.h) : 0;
}

/**
 * `! nothing covers the face`: each face on a frame, how much of it the visible text there covers (all of it: the
 * manifest has a box per word, and a caption covers a face as a line, not word by word), worst first; `faults` are the
 * ones over `limit` (proofBoxes' 8% of the face).
 */
export function textOverFace(text: Table<typeof FrameTextS>, faces: Table<typeof BoxS>, limit = 0.08) {
  const per = new Map<string, { frame: number; covers: number; text: string[] }>();
  for (const [t, f] of join(text, "frame", faces, "frame")) {
    if (t.opacity <= 0.5) continue;
    const c = covers(t, f);
    if (c <= 0) continue;
    const key = `${f.frame}:${f.x},${f.y}`;
    const e = per.get(key) ?? { frame: f.frame, covers: 0, text: [] };
    e.covers += c; e.text.push(t.text);
    per.set(key, e);
  }
  const all = [...per.values()].map((e) => ({ ...e, covers: Math.round(Math.min(1, e.covers) * 1000) / 1000, text: e.text.join(" ") })).sort((a, b) => b.covers - a.covers);
  return Object.assign(all.filter((e) => e.covers > limit), { worst: all[0] ?? null });
}

/** Read a compiled piece's facts from its space: NAME's graph, words, faces and render manifest, and the moments. */
export async function facts(space: string, name = "edit") {
  const read = async (p: string) => { try { return await Bun.file(p).text(); } catch { return null; } };
  const side = (f: string) => `${space}/${name === "edit" ? f : `${name}.${f}`}`;
  const g = await read(`${space}/.score/${name}.graph.json`);
  const words = await read(side("words.json"));
  const perception = await read(side("perception.json"));
  const manifest = await read(side("render.json"));
  const moments = await read(`${space}/.score/moments.jsonl`);
  const w = words ? wordsTable(JSON.parse(words)) : table(WordS, [], "word");
  const frames = w.length ? Math.max(...(w.column("to") as Uint32Array)) + 1 : 0;
  return {
    ...(g ? graphTables(JSON.parse(g)) : {}),
    words: w,
    faces: perception ? facesTable(JSON.parse(perception), frames) : table(BoxS, [], "face"),
    ...(manifest ? renderTables(JSON.parse(manifest)) : {}),
    moments: moments ? momentsTable(moments) : table(MomentS, [], "moment"),
  };
}
