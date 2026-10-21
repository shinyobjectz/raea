// @raea/data/check: a brief's `!` rules evaluated on the real video (the founder, 2026-10-17: "every part of a brief is
// checked against the real video through @raea/data"). The piece's facts are loaded as tables (facts.ts), faces are
// read back from the rendered frames, and each rule is a query over them.
//
//   bun ~/raea/sdk/data/src/check.ts NAME SPACE [--video PATH] [--battery]    (raea-brief check NAME SPACE runs this)
//   --battery adds a standard set to the piece's own rules: the 7 musts, speaking -> face, claims sourced, no shot longer
//   than 6 s, a cut at least every 4 s
//
// Reads SPACE/NAME.tsx's sidecars (NAME.words.json, NAME.perception.json, NAME.checks.json), SPACE/.score/NAME*.graph.json
// and NAME.ir.json, the render manifest NAME.render.json and the video (NAME.mp4; final.mp4 for `edit`; render.mp4; or
// --video); faces at 2 fps are cached in SPACE/.score/NAME.renderfaces.jsonl, perception's moments in moments.jsonl.
// Prints one line a rule, writes SPACE/NAME.check.json, exits 0 when every rule passes, 1 on a fail, 2 when it can't check.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { spawnSync } from "child_process";
import { BoxS, covers, momentsTable, renderTables, textOverFace, wordsTable } from "./facts";
import { table } from "./index";

const HOME = process.env.HOME ?? "";
const PERCEIVE = join(HOME, "raea/perception/target/release/raea-perceive");
const EMBED = join(HOME, "raea/meaning/target/release/raea-embed");
const FIND_THRESHOLD = 0.10; // SigLIP 2 text-image (engine/rdb/src/assets.rs)
const MUSTS = ["the hook reads within half a second", "nothing covers the face", "captions read on a phone",
  "everything inside the safe zone", "ends on a word, never mid-word", "no black frames", "the length is within the format"];
// the phrase SigLIP is asked for, for a find with no detector in ingest
const SIGLIP: Record<string, string> = { product: "a product", logo: "a brand logo", action: "a person doing something with their hands", hands: "hands", place: "a place, a room or a location" };

export interface Result { rule: string; form: string; status: "pass" | "fail" | "not measured"; frames: number[]; why: string }

const norm = (s: string) => s.toLowerCase().replace(/[’']/g, "").replace(/[^a-z0-9$% ]+/g, " ").split(/\s+/).filter(Boolean);
const read = (p: string) => (existsSync(p) ? readFileSync(p, "utf8") : null);
const json = (p: string) => { const t = read(p); return t ? JSON.parse(t) : null; };

/** the faces in the rendered video, 2 a second: [{frame, boxes in edit px}] (cached beside the piece) */
function renderFaces(video: string, fps: number, W: number, H: number, cache: string) {
  if (!existsSync(cache)) {
    const dir = mkdtempSync(join(tmpdir(), "raea-check-"));
    spawnSync("ffmpeg", ["-v", "error", "-i", video, "-vf", "fps=2", join(dir, "f%05d.jpg")]);
    const stills = readdirSync(dir).filter((f) => f.endsWith(".jpg")).sort().map((f) => join(dir, f));
    const out = stills.length ? spawnSync(PERCEIVE, ["faces", ...stills], { maxBuffer: 1 << 28 }).stdout.toString() : "";
    const rows = out.trim().split("\n").filter(Boolean).map((l) => {
      const r = JSON.parse(l); const n = Number(r.file.match(/f(\d+)\.jpg/)[1]);
      return { frame: Math.round(((n - 1) * fps) / 2), faces: r.faces.map((f: any) => [f.box[0] * W, f.box[1] * H, f.box[2] * W, f.box[3] * H]) };
    });
    writeFileSync(cache, rows.map((r) => JSON.stringify(r)).join("\n") + "\n");
    rmSync(dir, { recursive: true, force: true });
  }
  return read(cache)!.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) as { frame: number; faces: number[][] }[];
}

function blackFrames(video: string, fps: number): number[] {
  const r = spawnSync("ffmpeg", ["-v", "info", "-i", video, "-vf", "blackdetect=d=0.1:pix_th=0.10", "-an", "-f", "null", "-"]);
  const out = r.stderr.toString();
  const frames: number[] = [];
  for (const m of out.matchAll(/black_start:([\d.]+) black_end:([\d.]+)/g)) {
    for (let f = Math.round(Number(m[1]) * fps); f < Math.round(Number(m[2]) * fps); f++) frames.push(f);
  }
  return frames;
}

function embed(phrases: string[]): Record<string, Float32Array> {
  if (!phrases.length || !existsSync(EMBED)) return {};
  const out = spawnSync(EMBED, ["text", ...phrases]).stdout.toString();
  const r: Record<string, Float32Array> = {};
  for (const l of out.trim().split("\n").filter(Boolean)) { const d = JSON.parse(l); r[d.text] = Float32Array.from(d.v); }
  return r;
}

/** the graph a piece was compiled from: SPACE/.score/NAME.graph.json, or its stem's for NAME-A-9x16 */
function graphOf(space: string, name: string): any {
  let n = name;
  while (true) {
    const g = json(join(space, ".score", `${n}.graph.json`));
    if (g) return g;
    if (!n.includes("-")) return null;
    n = n.slice(0, n.lastIndexOf("-"));
  }
}

export async function check(space: string, name: string, video?: string, opts: { battery?: boolean } = {}): Promise<{ results: Result[]; code: number; said: string }> {
  const side = (f: string) => join(space, name === "edit" ? f : `${name}.${f}`);
  const manifest = json(side("render.json"));
  const vid = video ?? [join(space, `${name}.mp4`), name === "edit" ? join(space, "final.mp4") : "", join(space, "render.mp4")].find((p) => p && existsSync(p));
  if (!manifest || !vid) return { results: [], code: 2, said: `can't check ${name}: ${!manifest ? `no ${side("render.json")} (render with --manifest)` : "no rendered video"}` };
  const fps: number = manifest.fps ?? 30, W: number = manifest.width ?? 1080, H: number = manifest.height ?? 1920;
  const frames: number = (manifest.frames ?? []).length;
  const g = graphOf(space, name) ?? {};
  const ir = json(join(space, ".score", `${name}.ir.json`));
  // the piece's rules and those of the briefs it uses, each once
  const own: any[] = (json(side("checks.json")) ?? g.checks ?? []).filter((r: any, i: number, all: any[]) => all.findIndex((x) => x.rule.toLowerCase().replace(/\.$/, "") === r.rule.toLowerCase().replace(/\.$/, "")) === i);
  // --battery: a standard set beside the piece's own, the same for every piece (to compare pieces, or a judge)
  const battery = opts.battery ? [...MUSTS.map((m) => ({ rule: m, form: "must", args: [m] })), { rule: "speaking -> face", form: "implies", args: ["speaking", "face"] },
    { rule: "claims sourced", form: "claims", args: [] }, { rule: "no shot longer than 6 s", form: "max-shot", args: ["6"] }, { rule: "a cut at least every 4 s", form: "cut-every", args: ["4"] }] : [];
  const rules: any[] = [...own, ...battery.filter((b) => !own.some((o) => o.rule.toLowerCase().replace(/\.$/, "") === b.rule))];
  const words = wordsTable(json(side("words.json")) ?? []);
  const perception = json(side("perception.json")) ?? {};
  const { text, videos } = renderTables(manifest);
  mkdirSync(join(space, ".score"), { recursive: true });
  // perception's moments of the takes, for the facts the render itself can't say (eye contact, framing, motion, light)
  // only the takes the render shows (the moments of a take mean ingesting it once)
  const srcs = [...new Set(videos.toArray().map((v) => v.src))];
  const mpath = join(space, ".score", "moments.jsonl");
  const have = new Set((read(mpath) ?? "").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l).file));
  if (!existsSync(mpath) || srcs.some((s) => ![...have].some((f) => { try { return realpathSync(join(space, f)) === realpathSync(s); } catch { return false; } }))) {
    spawnSync("uv", ["run", "-q", "python", "-m", "compiler.moments", space, "--src", srcs.join(",")], { cwd: join(HOME, "raea/score") });
  }
  const moments = momentsTable(read(join(space, ".score", "moments.jsonl")) ?? "");
  const faces = renderFaces(vid, fps, W, H, join(space, ".score", `${name}.renderfaces.jsonl`));
  const faceBoxes = table(BoxS, [], "face");
  for (const r of faces) for (const b of r.faces) faceBoxes.push({ frame: r.frame, x: b[0], y: b[1], w: b[2], h: b[3] });
  const samples = faces.map((r) => r.frame).filter((f) => f < frames);

  // ---- what each frame shows ----
  const textAt = new Map<number, any[]>();
  for (const t of text.toArray()) if (t.opacity > 0.5) (textAt.get(t.frame) ?? textAt.set(t.frame, []).get(t.frame)!).push(t);
  const videoAt = new Map<number, any>();
  for (const v of videos.toArray()) { const cur = videoAt.get(v.frame); if (!cur || v.w * v.h * v.opacity > cur.w * cur.h * cur.opacity) videoAt.set(v.frame, v); }
  const ws = words.toArray();
  const saidAt = (f: number) => ws.some((w) => w.from <= f && f < w.to);
  // shots: a new one where the main video's source changes or its time jumps
  const cuts: number[] = [];
  for (let f = 1; f < frames; f++) {
    const a = videoAt.get(f - 1), b = videoAt.get(f);
    if (!a || !b) { if (!!a !== !!b) cuts.push(f); continue; }
    if (a.src !== b.src || Math.abs(b.at - a.at - 1 / fps) > 1.5 / fps) cuts.push(f);
  }
  // perception's moment for the source frame on screen
  const real = new Map<string, string>();
  const realOf = (p: string) => { if (!real.has(p)) { try { real.set(p, realpathSync(p)); } catch { real.set(p, p); } } return real.get(p)!; };
  const ms = moments.toArray();
  const msBySrc = new Map<string, any[]>();
  const momentRows = (read(join(space, ".score", "moments.jsonl")) ?? "").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  ms.forEach((m, i) => { const k = realOf(join(space, m.file)); (msBySrc.get(k) ?? msBySrc.set(k, []).get(k)!).push({ ...m, siglip: momentRows[i]?.siglip }); });
  const momentAt = (f: number) => {
    const v = videoAt.get(f); if (!v) return null;
    const list = msBySrc.get(realOf(v.src)); if (!list) return null;
    return list.find((m) => m.from <= v.at && v.at < m.to + 0.0625) ?? null;
  };
  const vecs = embed([...new Set(rules.filter((r) => r.form === "implies").flatMap((r) => r.args).filter((a: string) => SIGLIP[a]).map((a: string) => SIGLIP[a]))]);
  const cols = new Map<string, Float32Array>();
  const siglipScore = (m: any, phrase: string) => {
    if (!m?.siglip || !vecs[phrase]) return null;
    const [col, i] = m.siglip;
    if (!cols.has(col)) cols.set(col, existsSync(col) ? new Float32Array(readFileSync(col).buffer.slice(0)) : new Float32Array());
    const c = cols.get(col)!, q = vecs[phrase], d = q.length;
    if (c.length < (i + 1) * d) return null;
    let s = 0; for (let k = 0; k < d; k++) s += c[i * d + k] * q[k];
    return s;
  };
  /** whether a fact holds at a frame: true, false, or null when nothing can say */
  const fact = (name: string, f: number): boolean | null => {
    const nf = faces.find((r) => r.frame === f)?.faces.length ?? null;
    if (name === "face") return nf === null ? null : nf >= 1;
    if (name.startsWith("people:")) { const [lo, hi] = name.slice(7).split("-").map(Number); return nf === null ? null : nf >= lo && nf <= (hi ?? lo); }
    if (name === "speaking" || name === "speech") return saidAt(f);
    if (name === "text") return (textAt.get(f) ?? []).length > 0;
    if (name === "no-text") return (textAt.get(f) ?? []).length === 0;
    const m = momentAt(f);
    if (!m) return null;
    if (name === "eye-contact") return m.eye;
    if (name === "still" || name === "moving") return m.motion === name;
    if (name === "bright") return m.luma >= 0.45;
    if (name === "screen") return m.screen;
    if (name.startsWith("framing:")) return m.framing === name.slice(8);
    if (SIGLIP[name]) { const s = siglipScore(m, SIGLIP[name]); return s === null ? null : s >= FIND_THRESHOLD; }
    return null;
  };

  // ---- the rules ----
  const sec = (f: number) => `${(f / fps).toFixed(1)}s`;
  const list = (fs: number[]) => fs.length ? fs.slice(0, 8).map(sec).join(" ") + (fs.length > 8 ? ` (+${fs.length - 8})` : "") : "";
  const deliv = (() => { const ds = g.deliverables ?? []; const m = name.match(/-(\d+)x(\d+)$/); return (m && ds.find((d: any) => d.ratio === `${m[1]}:${m[2]}`)) || ds[0] || {}; })();
  const safe = (perception[Object.keys(perception)[0]] ?? {}).safe ?? { top: 0, bottom: H };
  const results: Result[] = [];
  const add = (r: any, status: Result["status"], frames: number[], why: string) => results.push({ rule: r.rule, form: r.form, status, frames, why });
  const lineSection: string[] = (g.items ?? []).filter((i: any) => i.type === "line").map((i: any) => i.section ?? "");
  for (const r of rules) {
    const must = r.form === "must" ? r.args[0] : null;
    if (must === MUSTS[0]) {
      const early = Math.ceil(fps / 2);
      const t = [...textAt.keys()].some((f) => f < early), w = ws.some((x) => x.from < early);
      add(r, t || w ? "pass" : "fail", t || w ? [] : [0], t ? "text is on screen in the first half second" : w ? "the first word is said in the first half second" : "nothing is shown or said in the first half second");
    } else if (must === MUSTS[1]) {
      const sampled = new Set(samples);
      const bad = textOverFace(text.where((t) => sampled.has(t.frame)), faceBoxes);
      const worst = (bad as any).worst;
      add(r, bad.length ? "fail" : "pass", [...new Set(bad.map((b) => b.frame))], bad.length ? `text covers a face over 8% at ${list([...new Set(bad.map((b) => b.frame))])} (worst ${Math.round(bad[0].covers * 100)}%: "${bad[0].text}")` : `the most text covers a face is ${worst ? Math.round(worst.covers * 1000) / 10 : 0}% (faces read back from the render)`);
    } else if (must === MUSTS[2]) {
      const bad = [...textAt.entries()].filter(([, ts]) => ts.some((t) => t.h < 0.025 * H)).map(([f]) => f).sort((a, b) => a - b);
      add(r, bad.length ? "fail" : "pass", bad, bad.length ? `text under ${Math.round(0.025 * H)} px tall at ${list(bad)}` : `every text at least ${Math.round(0.025 * H)} px tall`);
    } else if (must === MUSTS[3]) {
      const bad = [...textAt.entries()].filter(([, ts]) => ts.some((t) => t.x < 0 || t.x + t.w > W || t.y < safe.top || t.y + t.h > safe.bottom)).map(([f]) => f).sort((a, b) => a - b);
      add(r, bad.length ? "fail" : "pass", bad, bad.length ? `text outside ${safe.top}-${safe.bottom} px (the platforms' UI) or off the frame at ${list(bad)}` : `every text inside ${safe.top}-${safe.bottom} px and the frame`);
    } else if (must === MUSTS[4]) {
      const bad = [...cuts, frames].filter((c) => ws.some((w) => w.from < c - 1 && c + 1 < w.to));
      add(r, bad.length ? "fail" : "pass", bad, bad.length ? `a word runs through the cut at ${list(bad)}` : `${cuts.length} cuts and the end, none inside a word`);
    } else if (must === MUSTS[5]) {
      const bad = blackFrames(vid, fps);
      add(r, bad.length ? "fail" : "pass", bad, bad.length ? `black at ${list(bad)}` : "no black frames (ffmpeg blackdetect)");
    } else if (must === MUSTS[6]) {
      const secs = frames / fps, [lo, hi] = r.args[1] ? [Number(r.args[1]) * 0.85, Number(r.args[1]) * 1.15] : deliv.secs ?? [0, Infinity];
      add(r, secs >= lo && secs <= hi ? "pass" : "fail", [], `${secs.toFixed(1)} s against ${lo.toFixed(1)}-${hi.toFixed(1)} s`);
    } else if (r.form === "never-says") {
      const said = ` ${ws.flatMap((w) => norm(w.text)).join(" ")} `;
      const hit = r.args.filter((a: string) => said.includes(` ${norm(a).join(" ")} `));
      const where = hit.length ? ws.filter((w, i) => hit.some((h: string) => norm(ws.slice(i, i + norm(h).length).map((x) => x.text).join(" ")).join(" ") === norm(h).join(" "))).map((w) => w.from) : [];
      add(r, hit.length ? "fail" : "pass", where, hit.length ? `says "${hit.join('", "')}" at ${list(where)}` : "never said");
    } else if (r.form === "implies") {
      const [a, b] = r.args;
      let measured = 0, holds = 0, unread = 0; const bad: number[] = [];
      for (const f of samples) {
        const fa = fact(a, f); if (fa !== true) { if (fa === null) unread++; continue; }
        holds++;
        const fb = fact(b, f); if (fb === null) continue;
        measured++; if (!fb) bad.push(f);
      }
      if (!measured) add(r, "not measured", [], holds ? `${a} holds on ${holds} samples, but ${b} can't be read there (no ${SIGLIP[b] ? "SigLIP row" : "moment"} for the source on screen)`
        : unread === samples.length ? `${a} can't be read on any sample (no moments for the takes on screen)` : `${a} never holds on the ${samples.length} samples, so there's nothing to check`);
      else add(r, bad.length ? "fail" : "pass", bad, bad.length ? `${a} without ${b} at ${list(bad)} (${bad.length} of ${measured} samples)` : `${b} whenever ${a}, ${measured} samples`);
    } else if (r.form === "claims") {
      const claims = g.claims ?? {};
      const used = (g.items ?? []).filter((i: any) => i.type === "line" && (i.claims ?? []).length);
      const unsourced = used.flatMap((i: any) => i.claims.filter((c: string) => !claims[c]?.source).map((c: string) => ({ c, text: i.text })));
      // where an unsourced claim is said or shown in the render
      const at: number[] = [];
      for (const u of unsourced) {
        const q = norm(claims[u.c]?.text ?? u.text).join(" ");
        ws.forEach((w, i) => { if (q && norm(ws.slice(i, i + q.split(" ").length).map((x) => x.text).join(" ")).join(" ") === q) at.push(w.from); });
        for (const [f, ts] of textAt) if (q && norm(ts.map((t: any) => t.text).join(" ")).join(" ").includes(q)) at.push(f);
      }
      add(r, unsourced.length ? "fail" : "pass", [...new Set(at)].sort((x, y) => x - y), unsourced.length ? `no source for ${unsourced.map((u: any) => `claim:${u.c}`).join(", ")}${at.length ? `, said or shown at ${list([...new Set(at)].sort((x, y) => x - y))}` : ""}` : used.length ? `${used.length} claimed line(s), each sourced` : "no claims made");
    } else if (r.form === "max-shot") {
      const bounds = [0, ...cuts, frames];
      let worst = 0, at = 0;
      for (let i = 1; i < bounds.length; i++) if (bounds[i] - bounds[i - 1] > worst) { worst = bounds[i] - bounds[i - 1]; at = bounds[i - 1]; }
      add(r, worst / fps <= Number(r.args[0]) ? "pass" : "fail", worst / fps <= Number(r.args[0]) ? [] : [at], `the longest shot is ${(worst / fps).toFixed(1)} s (from ${sec(at)})`);
    } else if (r.form === "hook-by") {
      const clips = ir?.clips ?? [];
      const first = lineSection[0];
      const ends = clips.filter((c: any) => c.line >= 0 && lineSection[c.line] === first).map((c: any) => c.end);
      if (!ends.length) add(r, "not measured", [], "no clip of the first section in the IR");
      else { const end = Math.max(...ends); add(r, end / fps <= Number(r.args[0]) ? "pass" : "fail", end / fps <= Number(r.args[0]) ? [] : [end], `the first section (${first}) ends at ${sec(end)}`); }
    } else if (r.form === "cut-every") {
      const onsets: number[] = [];
      for (let f = 1; f < frames; f++) {
        const before = new Set((textAt.get(f - 1) ?? []).map((t) => t.text)), now = (textAt.get(f) ?? []).map((t) => t.text);
        if (now.length && !before.size) onsets.push(f);
      }
      const events = [0, ...new Set([...cuts, ...onsets].sort((a, b) => a - b)), frames];
      let worst = 0, at = 0;
      for (let i = 1; i < events.length; i++) if (events[i] - events[i - 1] > worst) { worst = events[i] - events[i - 1]; at = events[i - 1]; }
      add(r, worst / fps <= Number(r.args[0]) ? "pass" : "fail", worst / fps <= Number(r.args[0]) ? [] : [at], `the longest stretch with no cut and no new text is ${(worst / fps).toFixed(1)} s (from ${sec(at)})`);
    } else {
      add(r, "not measured", [], `no evaluator for the form ${r.form}`);
    }
  }
  writeFileSync(side("check.json"), JSON.stringify(results, null, 1));
  const said = results.map((x) => `${x.status.toUpperCase().padEnd(12)} ! ${x.rule}: ${x.why}`).join("\n");
  return { results, code: results.some((x) => x.status === "fail") ? 1 : 0, said };
}

if (import.meta.main) {
  const a = process.argv.slice(2);
  const vi = a.indexOf("--video");
  const video = vi >= 0 ? a[vi + 1] : undefined;
  const [name, space] = a.filter((x, i) => x !== "--video" && x !== "--battery" && (vi < 0 || i !== vi + 1));
  const r = await check(space, name, video, { battery: a.includes("--battery") });
  console.log(r.said);
  process.exit(r.code);
}
