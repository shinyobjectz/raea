// @raea/data/project: a project's record that outlives a turn (sdk/data/project.feature; the founder, 2026-10-17: "project
// management, giving the agent an SOP, committing work or saving work, and managing tests and when to present"). The
// project is the folder void works in (the workspace); it holds objects, each anything that renders (an ad, a pong game,
// a deck), in a folder of its own (objects/<slug>/, or an older workspace's spaces/<slug>/; a lone folder with no
// objects in it is one object itself, as an eval's is). The cube at the stage's top-left switches between them. Requests
// are tasks, with steps, questions the person answers by choosing, waivers and a state; notes keep what the person
// decided. The project's record is ROOT/.raea/project.jsonl (an append-only event log, folded on read); each object's
// versions are OBJECT/.raea/versions.jsonl, its files and renders kept by content hash in OBJECT/.raea/objects (the
// action log's own store: sha256 of the bytes), so the person's own edits and host-written files are kept too.
//
//   bun ~/raea/sdk/data/src/project.ts CMD ROOT ARGS... [--json]     (the `raea` commands shell to this; RAEA_DATA overrides)
//   status                                          the few lines a turn starts with
//   purpose "text"                                  what the project is for
//   objects                                         each object: its kind, state, last version, the one on screen
//   object open SLUG; object kind SLUG KIND         the object on screen; what it is (video, game, deck, document,
//                                                   music, motion, scene3d, still), kept in its space.json
//   task new "words" [--kind K] [--object O] [--accept "! rule"]...    -> the task's id (t1, t2, ...)
//   task step ID "text" [--done]; task state ID STATE [--waive "rule: reason"]...; task show [ID]; tasks
//   ask ID "question" --option "label: meaning"... [--multi]           -> the question's id (q1, ...)
//   answer QID "label"[,"label"] | "other: words"
//   note "text" [--rule "! ..."]; notes
//   save [--object O] [--message M] [--render FILE] [--check FILE] [--full] [--task ID]   -> version N
//   versions [--object O]; diff A B [--object O]; checkout N DIR [--object O] (version N's files and render into DIR,
//                                                   the object untouched: play an old one)
//   restore N [--object O] [--paths]                writes the object back as version N (--paths: what it would write,
//                                                   for the host to keep in the action log before it runs)
// A task is ready only when its object's last version had a full check, and every failing rule and every acceptance rule
// the check didn't measure is waived in writing; `task state ID ready` refuses otherwise and says what's missing.

import { createHash } from "crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "fs";
import { basename, dirname, join } from "path";

export const KINDS = ["video", "game", "deck", "document", "music", "motion", "scene3d", "still"] as const;
export const TASK_KINDS = ["new object", "change", "quick edit", "question"] as const;
export const STATES = ["planned", "working", "needs input", "draft", "ready", "delivered"] as const;
export type State = (typeof STATES)[number];

export interface Obj { name: string; kind: string; title: string; dir: string; entry: string }
export interface Option { label: string; meaning: string }
export interface Question { id: string; task: string; text: string; options: Option[]; multi: boolean; answer: string[] | null; other: string | null }
export interface Step { text: string; done: boolean }
export interface Waiver { rule: string; reason: string }
export interface Task { id: string; words: string; kind: string; object: string; accept: string[]; steps: Step[]; state: State; waivers: Waiver[]; versions: number[]; questions: string[]; at: number; marked: number | null }
export interface Note { text: string; rule: string | null; at: number }
export interface Check { full: boolean; total: number; fails: string[]; measured: string[] }
export interface Version { n: number; at: number; object: string; message: string; task: string | null; seq: number; state: string; files: Record<string, string>; render: { path: string; hash: string } | null; check: Check | null }
export interface Project { root: string; purpose: string; objects: Obj[]; current: string | null; tasks: Task[]; questions: Question[]; notes: Note[] }

const META = ".raea";
// not the work: perception's media, the records, the compiler's caches, generated sound and pictures (rdb's store in
// assets/, a cache by what was asked: a restore mustn't roll it back), packages, the app's own state, renders (a version
// keeps its render by hash instead)
const SKIP_DIRS = new Set(["media", "assets", META, ".void", ".score", ".looks", ".stills", ".sheets", "node_modules", ".git", "frames"]);
const SKIP_FILES = new Set(["space.json", "terminal.json", ".DS_Store"]);
const RENDER = /\.(mp4|mov|webm|m4a|wav)$/i;

const lines = (p: string) => (existsSync(p) ? readFileSync(p, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l)) : []);
const json = (p: string) => { try { return JSON.parse(readFileSync(p, "utf8")); } catch { return null; } };
const hashOf = (b: Buffer) => createHash("sha256").update(b).digest("hex");

/** Keeps the bytes in DIR/.raea/objects by their hash (as the action log does); the hash. */
function keep(dir: string, b: Buffer): string {
  const h = hashOf(b), o = join(dir, META, "objects", h);
  if (!existsSync(o)) { mkdirSync(dirname(o), { recursive: true }); writeFileSync(o, b); }
  return h;
}

function log(root: string, ev: Record<string, unknown>) {
  mkdirSync(join(root, META), { recursive: true });
  appendFileSync(join(root, META, "project.jsonl"), JSON.stringify({ at: Date.now(), ...ev }) + "\n");
}

/** The objects of a project: each folder under objects/ or spaces/ with a space.json, or the folder itself when it
 * holds none. */
export function objectsOf(root: string): Obj[] {
  const of = (dir: string): Obj => {
    const sj = json(join(dir, "space.json")) ?? {};
    return { name: sj.slug ?? basename(dir), kind: sj.kind ?? "video", title: sj.name ?? basename(dir), dir, entry: "edit.tsx" };
  };
  const found: Obj[] = [];
  for (const sub of ["objects", "spaces"]) {
    const d = join(root, sub);
    if (!existsSync(d)) continue;
    for (const e of readdirSync(d, { withFileTypes: true })) if (e.isDirectory() && existsSync(join(d, e.name, "space.json"))) found.push(of(join(d, e.name)));
  }
  // the legacy layout (void's global folder): objects straight under the root
  if (!found.length) for (const e of readdirSync(root, { withFileTypes: true })) if (e.isDirectory() && existsSync(join(root, e.name, "space.json")) && existsSync(join(root, e.name, "edit.tsx"))) found.push(of(join(root, e.name)));
  if (found.length) return found.sort((a, b) => a.name.localeCompare(b.name));
  return existsSync(join(root, "edit.tsx")) || existsSync(join(root, "space.json")) ? [of(root)] : [];
}

/** Every tracked file of an object, as /path -> hash (each kept). */
export function snapshot(dir: string): Record<string, string> {
  const out: Record<string, string> = {};
  const walk = (d: string, v: string) => {
    for (const e of readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const p = join(d, e.name), pv = `${v}/${e.name}`;
      if (e.isDirectory()) { if (!SKIP_DIRS.has(e.name)) walk(p, pv); continue; }
      if (e.isSymbolicLink() || SKIP_FILES.has(e.name) || RENDER.test(e.name)) continue;
      if (e.isFile()) out[pv] = keep(dir, readFileSync(p));
    }
  };
  walk(dir, "");
  return out;
}

/** An object's versions (a later row with the same number replaces it: a check added to a version). */
export function versionsOf(dir: string): Version[] {
  const by = new Map<number, Version>();
  for (const v of lines(join(dir, META, "versions.jsonl"))) by.set(v.n, v);
  return [...by.values()].sort((a, b) => a.n - b.n);
}

/** The project's record, folded from its log. */
export function load(root: string): Project {
  const pr: Project = { root, purpose: "", objects: objectsOf(root), current: null, tasks: [], questions: [], notes: [] };
  const task = (id: string) => pr.tasks.find((t) => t.id === id);
  for (const e of lines(join(root, META, "project.jsonl"))) {
    switch (e.ev) {
      case "purpose": pr.purpose = e.text; break;
      case "open": pr.current = e.object; break;
      case "task": pr.tasks.push({ id: e.id, words: e.words, kind: e.kind, object: e.object, accept: e.accept ?? [], steps: [], state: "planned", waivers: [], versions: [], questions: [], at: e.at, marked: null }); break;
      case "retarget": { const t = task(e.id); if (t) t.object = e.object; break; }
      case "step": { const t = task(e.id); if (!t) break; const s = t.steps.find((x) => x.text === e.text); s ? (s.done = e.done) : t.steps.push({ text: e.text, done: e.done }); break; }
      case "state": { const t = task(e.id); if (t) { t.state = e.state; t.marked = e.version ?? null; t.waivers.push(...(e.waive ?? [])); } break; }
      case "ask": pr.questions.push({ id: e.q, task: e.id, text: e.text, options: e.options, multi: e.multi, answer: null, other: null }); task(e.id)?.questions.push(e.q); break;
      case "answer": { const q = pr.questions.find((x) => x.id === e.q); if (q) { q.answer = e.answer; q.other = e.other; } break; }
      case "note": pr.notes.push({ text: e.text, rule: e.rule ?? null, at: e.at }); break;
    }
  }
  if (!pr.objects.some((o) => o.name === pr.current)) pr.current = pr.objects[0]?.name ?? null;
  for (const o of pr.objects) for (const v of versionsOf(o.dir)) if (v.task) task(v.task)?.versions.push(v.n);
  return pr;
}

/** An object by its slug or title; the one on screen when none is named. */
export function objectOf(pr: Project, name?: string): Obj {
  const n = name?.trim().toLowerCase();
  const o = n ? pr.objects.find((x) => x.name.toLowerCase() === n || x.title.toLowerCase() === n) : pr.objects.find((x) => x.name === pr.current);
  if (!o) throw new Error(n ? `no object ${name} (objects: ${pr.objects.map((x) => x.name).join(", ") || "none"})` : "no object yet");
  return o;
}

const lastOf = (pr: Project, object: string) => { const o = pr.objects.find((x) => x.name === object); return o ? versionsOf(o.dir).at(-1) ?? null : null; };

/** What stands between a task and ready: the last version's failing rules, and acceptance rules no check measured, each
 * not waived; a fast check only (the full one wasn't run); no version at all. */
export function readiness(pr: Project, t: Task, waive: Waiver[] = []) {
  const v = lastOf(pr, t.object);
  const waived = new Set([...t.waivers, ...waive].map((w) => norm(w.rule)));
  if (!v) return { ok: false, version: null, missing: [`no version of ${t.object} yet (raea render saves one)`] };
  if (!v.check) return { ok: false, version: v.n, missing: [`v${v.n} wasn't checked (raea check --full)`] };
  const missing: string[] = [];
  if (!v.check.full) missing.push(`v${v.n} had the fast check only (raea check --full)`);
  for (const f of v.check.fails) if (!waived.has(norm(f))) missing.push(`fails: ${f}`);
  const measured = new Set(v.check.measured.map(norm));
  for (const a of t.accept) if (!measured.has(norm(a)) && !waived.has(norm(a))) missing.push(`not measured: ${a}`);
  return { ok: !missing.length, version: v.n, missing };
}
const norm = (r: string) => r.toLowerCase().replace(/^!\s*/, "").replace(/\.$/, "").trim();

/** An object's state: no render, draft, ready, delivered (ready and delivered hold for the version they were given). */
export function objectState(pr: Project, object: string): string {
  const v = lastOf(pr, object);
  if (!v) return "no render";
  const t = pr.tasks.filter((x) => x.object === object && (x.state === "ready" || x.state === "delivered") && x.marked === v.n).at(-1);
  return t ? t.state : "draft";
}

/** A check file (data/check.ts's NAME.check.json, or raea check's {full, results}) as what a version keeps. */
export function readCheck(path: string, full: boolean): Check {
  const j = JSON.parse(readFileSync(path, "utf8"));
  const rows: any[] = Array.isArray(j) ? j : j.results ?? j.faults ?? j.checks ?? [];
  const rule = (r: any) => (typeof r === "string" ? r : r.rule ?? r.check ?? r.what ?? JSON.stringify(r));
  const failed = (r: any) => typeof r === "string" || r.status === "fail" || r.ok === false || r.pass === false;
  return { full: full || !!j.full, total: rows.length, fails: rows.filter(failed).map(rule), measured: rows.filter((r) => typeof r !== "string" && r.status !== "not measured").map(rule) };
}

/** Saves a version of an object: its files, the render and its check. An unchanged object and render give the last
 * version back, with the check put on it when one comes. */
export function save(root: string, o: { object?: string; message?: string; render?: string; check?: string; full?: boolean; task?: string } = {}): { version: Version; fresh: boolean } {
  const pr = load(root);
  const ob = objectOf(pr, o.object);
  const dir = ob.dir;
  const files = snapshot(dir);
  const at = (p: string) => (p.startsWith("/") ? p : join(dir, p));
  const rp = o.render ? at(o.render) : null;
  const render = rp && existsSync(rp) ? { path: rp.startsWith(dir + "/") ? rp.slice(dir.length) : rp, hash: keep(dir, readFileSync(rp)) } : null;
  const check = o.check ? readCheck(at(o.check), !!o.full) : null;
  const last = lines(join(dir, META, "actions.jsonl")).at(-1), prev = versionsOf(dir).at(-1);
  const same = !!prev && JSON.stringify(prev.files) === JSON.stringify(files) && (!render || prev.render?.hash === render.hash);
  if (same && !check) return { version: prev!, fresh: false };
  const task = o.task ?? pr.tasks.filter((t) => t.object === ob.name && t.state !== "delivered").at(-1)?.id ?? null;
  const v: Version = same ? { ...prev!, check, message: o.message ?? prev!.message } : { n: (prev?.n ?? 0) + 1, at: Date.now(), object: ob.name, message: o.message ?? (render ? `render of ${ob.title}` : `saved ${ob.title}`), task,
    seq: last?.seq ?? 0, state: last?.state_after ?? "0".repeat(64), files, render: render ?? (prev && JSON.stringify(prev.files) === JSON.stringify(files) ? prev.render : null), check };
  mkdirSync(join(dir, META), { recursive: true });
  appendFileSync(join(dir, META, "versions.jsonl"), JSON.stringify(v) + "\n");
  return { version: v, fresh: !same };
}

/** What changed from version a to b of an object: files added, removed and changed, and whether the render did. */
export function diff(dir: string, a: number, b: number) {
  const vs = versionsOf(dir), A = vs.find((v) => v.n === a), B = vs.find((v) => v.n === b);
  if (!A || !B) throw new Error(`no version ${!A ? a : b} (${vs.length} saved)`);
  const keys = [...new Set([...Object.keys(A.files), ...Object.keys(B.files)])].sort();
  return { added: keys.filter((k) => !A.files[k] && B.files[k]), removed: keys.filter((k) => A.files[k] && !B.files[k]),
    changed: keys.filter((k) => A.files[k] && B.files[k] && A.files[k] !== B.files[k]), render: A.render?.hash !== B.render?.hash };
}

const versionN = (dir: string, n: number) => { const v = versionsOf(dir).find((x) => x.n === n); if (!v) throw new Error(`no version ${n}`); return v; };

/** The writes that put the object's tracked files back as version n's: each path to its hash, or null to remove it. */
export function plan(dir: string, n: number): Record<string, string | null> {
  const v = versionN(dir, n), now = snapshot(dir);
  const out: Record<string, string | null> = {};
  for (const [p, h] of Object.entries(v.files)) if (now[p] !== h) out[p] = h;
  for (const p of Object.keys(now)) if (!(p in v.files)) out[p] = null;
  return out;
}

/** Writes version n's files into `into` (the object itself for a restore, whose current files are saved first as a
 * version when no version holds them, so a restore done outside the shell loses nothing either; any other folder for a
 * checkout, which gets the render too). The paths written. */
export function writeVersion(root: string, ob: Obj, n: number, into = ob.dir): string[] {
  const v = versionN(ob.dir, n);
  let writes: Record<string, string | null>;
  if (into === ob.dir) {
    const now = snapshot(ob.dir);
    if (!versionsOf(ob.dir).some((x) => JSON.stringify(x.files) === JSON.stringify(now))) save(root, { object: ob.name, message: `before restoring v${n}` });
    writes = plan(ob.dir, n);
  } else {
    writes = { ...v.files };
    if (v.render) writes[v.render.path.startsWith("/") && !v.render.path.startsWith(ob.dir) && v.render.path.split("/").length > 2 ? "/" + basename(v.render.path) : v.render.path] = v.render.hash;
  }
  for (const [p, h] of Object.entries(writes)) {
    const real = join(into, p);
    if (h === null) { rmSync(real, { force: true }); continue; }
    mkdirSync(dirname(real), { recursive: true });
    writeFileSync(real, readFileSync(join(ob.dir, META, "objects", h)));
  }
  return Object.keys(writes);
}

// ---- the words a turn starts with ----------------------------------------------------------------------------------

const ago = (t: number) => { const s = (Date.now() - t) / 1000; return s < 90 ? "just now" : s < 5400 ? `${Math.round(s / 60)} min ago` : s < 129600 ? `${Math.round(s / 3600)} h ago` : `${Math.round(s / 86400)} d ago`; };
export const checkLine = (c: Check | null) => (!c ? "not checked" : `${c.full ? "full" : "fast"} check ${c.total - c.fails.length}/${c.total}${c.fails.length ? ` (fails: ${c.fails.slice(0, 3).join("; ")}${c.fails.length > 3 ? "; ..." : ""})` : ""}`);

export function status(root: string): string {
  const pr = load(root);
  const out: string[] = [];
  out.push(`project: ${pr.purpose || "(no purpose written: raea purpose \"...\")"}`);
  if (pr.objects.length) {
    out.push("objects (* on screen):");
    for (const o of pr.objects) {
      const v = lastOf(pr, o.name);
      out.push(`  ${o.name === pr.current ? "*" : " "} ${o.name} (${o.kind}${o.title !== o.name ? `, "${o.title}"` : ""}): ${objectState(pr, o.name)}${v ? `, v${v.n} ${ago(v.at)}, ${checkLine(v.check)}` : ""}`);
    }
  }
  for (const t of pr.tasks.filter((t) => t.state !== "delivered" && t.state !== "ready")) {
    const next = t.steps.find((s) => !s.done);
    const waiting = pr.questions.filter((q) => q.task === t.id && !q.answer);
    out.push(`task ${t.id} [${t.state}] ${t.object}: "${t.words}"${next ? `; next: ${next.text}` : ""}${waiting.map((q) => `; waiting on ${q.id}: ${q.text}`).join("")}`);
    for (const q of pr.questions.filter((q) => q.task === t.id && q.answer)) out.push(`  ${q.id} "${q.text}" -> ${q.other ? `"${q.other}"` : q.answer!.join(", ")}`);
  }
  if (pr.notes.length) out.push(`notes: ${pr.notes.map((n) => n.text).join("; ")}`);
  return out.join("\n");
}

// ---- the command line ----------------------------------------------------------------------------------------------

function flags(a: string[]) {
  const pos: string[] = [], one: Record<string, string> = {}, many: Record<string, string[]> = {}, on = new Set<string>();
  const MANY = new Set(["option", "accept", "waive"]), BOOL = new Set(["done", "multi", "full", "paths", "json"]);
  for (let i = 0; i < a.length; i++) {
    const m = a[i].match(/^--([\w-]+)$/);
    if (!m) { pos.push(a[i]); continue; }
    if (BOOL.has(m[1])) on.add(m[1]);
    else if (MANY.has(m[1])) (many[m[1]] ??= []).push(a[++i]);
    else one[m[1]] = a[++i];
  }
  return { pos, one, many, on };
}
const waiver = (s: string): Waiver => { const i = s.lastIndexOf(":"); return i > 0 ? { rule: s.slice(0, i).trim(), reason: s.slice(i + 1).trim() } : { rule: s, reason: "" }; };
const num = (s?: string) => Number((s ?? "").replace(/^v/, ""));

export function run(argv: string[]): { out: unknown; text: string; code: number } {
  const [cmd, root, ...rest] = argv;
  const usage = (t: string) => ({ out: { error: t }, text: t, code: 2 });
  if (!cmd || !root) return usage("project.ts CMD ROOT ...  (status, purpose, objects, object, task, tasks, ask, answer, note, notes, save, versions, diff, checkout, restore)");
  const { pos, one, many, on } = flags(rest);
  const ok = (out: unknown, text: string) => ({ out, text, code: 0 });
  try {
    const pr = load(root);
    const task = (id: string) => { const t = pr.tasks.find((x) => x.id === id); if (!t) throw new Error(`no task ${id}`); return t; };
    switch (cmd) {
      case "status": return ok(pr, status(root));
      case "purpose": if (!pos[0]) return ok({ purpose: pr.purpose }, pr.purpose || "(none)"); log(root, { ev: "purpose", text: pos[0] }); return ok({ purpose: pos[0] }, `purpose: ${pos[0]}`);
      case "objects": {
        const rows = pr.objects.map((o) => { const v = lastOf(pr, o.name); return { ...o, state: objectState(pr, o.name), current: o.name === pr.current, last: v?.n ?? null, check: v ? checkLine(v.check) : null }; });
        return ok(rows, rows.map((o) => `${o.current ? "*" : " "} ${o.name} (${o.kind}) "${o.title}": ${o.state}${o.last ? `, v${o.last}, ${o.check}` : ""}`).join("\n") || "no objects");
      }
      case "object": {
        const [sub, name, kind] = pos;
        if (sub === "open") {
          const o = objectOf(pr, name);
          log(root, { ev: "open", object: o.name });
          return ok({ current: o.name, dir: o.dir }, `on screen: ${o.title}`);
        }
        if (sub === "kind") {
          const o = objectOf(pr, name);
          if (!(KINDS as readonly string[]).includes(kind)) return usage(`kind is one of ${KINDS.join(", ")}`);
          const p = join(o.dir, "space.json");
          writeFileSync(p, JSON.stringify({ ...(json(p) ?? { name: o.title, slug: o.name }), kind }, null, 2));
          return ok({ object: o.name, kind }, `${o.title} is a ${kind}`);
        }
        return usage("object open SLUG | object kind SLUG KIND");
      }
      case "task": {
        const [sub, a, b] = pos;
        if (sub === "new") {
          if (!a) return usage('task new "the person\'s words" [--kind K] [--object O] [--accept "! rule"]...');
          const kind = one.kind ?? "change";
          if (!(TASK_KINDS as readonly string[]).includes(kind)) return usage(`kind is one of ${TASK_KINDS.join(", ")}`);
          const id = `t${pr.tasks.length + 1}`;
          const object = one.object ? objectOf(pr, one.object).name : pr.current ?? "";
          log(root, { ev: "task", id, words: a, kind, object, accept: many.accept ?? [] });
          log(root, { ev: "state", id, state: "working", waive: [] });
          return ok({ id, object }, `${id} (for ${object || "no object yet"})`);
        }
        if (sub === "step") { task(a); if (!b) return usage('task step ID "text" [--done]'); log(root, { ev: "step", id: a, text: b, done: on.has("done") }); return ok({ id: a }, `${a}: ${on.has("done") ? "done" : "to do"}: ${b}`); }
        if (sub === "state") {
          const t = task(a);
          if (!(STATES as readonly string[]).includes(b)) return usage(`state is one of ${STATES.join(", ")}`);
          const waive = (many.waive ?? []).map(waiver);
          if (waive.some((w) => !w.reason)) return usage('a waiver says why: --waive "rule: the reason"');
          let version: number | null = null;
          if (b === "ready" || b === "delivered") {
            const r = readiness(pr, t, waive);
            version = r.version;
            if (!r.ok) return { out: r, text: `${t.id} isn't ready:\n${r.missing.map((m) => `  ${m}`).join("\n")}\nfix them, or waive each in writing (--waive "rule: reason"); until then it's a draft`, code: 1 };
          }
          if (b === "needs input" && !pr.questions.some((q) => q.task === t.id && !q.answer)) return usage(`${t.id} has no open question: ask one (raea ask ${t.id} "..." --option ...)`);
          log(root, { ev: "state", id: a, state: b, waive, version });
          return ok({ id: a, state: b, version }, `${a}: ${b}${version ? ` (v${version})` : ""}`);
        }
        if (sub === "object") { const t = task(a); const o = objectOf(pr, b); log(root, { ev: "retarget", id: a, object: o.name }); return ok({ id: a, object: o.name }, `${a} is for ${o.title}`); }
        if (sub === "show" || !sub) {
          const t = a ? task(a) : pr.tasks.at(-1);
          if (!t) return ok(null, "no tasks");
          const r = readiness(pr, t);
          return ok({ ...t, readiness: r }, [`${t.id} [${t.state}] ${t.kind}, ${t.object}: "${t.words}"`, ...t.accept.map((x) => `  accept: ${x}`), ...t.steps.map((s) => `  [${s.done ? "x" : " "}] ${s.text}`),
            ...t.questions.map((q) => { const x = pr.questions.find((y) => y.id === q)!; return `  ${q} ${x.text} -> ${x.answer ? x.other ?? x.answer.join(", ") : "(waiting)"}`; }),
            ...t.waivers.map((w) => `  waived ${w.rule}: ${w.reason}`), `  versions: ${t.versions.map((n) => `v${n}`).join(", ") || "none"}`, `  ready: ${r.ok ? "yes" : r.missing.join("; ")}`].join("\n"));
        }
        return usage("task new|step|state|object|show");
      }
      case "tasks": return ok(pr.tasks, pr.tasks.map((t) => `${t.id} [${t.state}] ${t.object}: "${t.words}"`).join("\n") || "no tasks");
      case "ask": {
        const [id, text] = pos;
        task(id);
        const options = (many.option ?? []).map((o) => { const i = o.indexOf(":"); return i > 0 ? { label: o.slice(0, i).trim(), meaning: o.slice(i + 1).trim() } : { label: o.trim(), meaning: "" }; });
        if (!text || options.length < 2 || options.length > 4) return usage('raea ask ID "question" --option "label: meaning" (2 to 4; the person can always answer in their own words) [--multi]');
        const q = `q${pr.questions.length + 1}`;
        log(root, { ev: "ask", id, q, text, options, multi: on.has("multi") });
        log(root, { ev: "state", id, state: "needs input", waive: [] });
        return ok({ id: q, task: id, text, options, multi: on.has("multi") }, `${q} asked: the person answers in the chat; end your turn now and wait`);
      }
      case "question": {
        const q = pos[0] ? pr.questions.find((x) => x.id === pos[0]) : pr.questions.filter((x) => !x.answer).at(-1);
        return ok(q ?? null, q ? `${q.id} ${q.text}\n${q.options.map((o) => `  ${o.label}: ${o.meaning}`).join("\n")}` : "no open question");
      }
      case "answer": {
        const [qid, a] = pos;
        const q = pr.questions.find((x) => x.id === qid);
        if (!q || a === undefined) return usage(`answer QID "label" | "other: words"  (${q ? "" : `no question ${qid}`})`);
        const other = /^other:/i.test(a) ? a.replace(/^other:\s*/i, "") : null;
        const answer = other !== null ? ["Other"] : a.split(",").map((s) => s.trim()).filter(Boolean);
        const bad = answer.filter((x) => x !== "Other" && !q.options.some((o) => o.label.toLowerCase() === x.toLowerCase()));
        if (bad.length) return usage(`${qid}'s options are ${q.options.map((o) => o.label).join(", ")}, or "other: your words"`);
        if (answer.length > 1 && !q.multi) return usage(`${qid} takes one answer`);
        const labels = answer.map((x) => q.options.find((o) => o.label.toLowerCase() === x.toLowerCase())?.label ?? x);
        log(root, { ev: "answer", q: qid, answer: labels, other });
        // the task goes back to work once nothing it asked is open
        if (!pr.questions.some((x) => x.task === q.task && x.id !== qid && !x.answer)) log(root, { ev: "state", id: q.task, state: "working", waive: [] });
        return ok({ id: qid, answer: labels, other, text: q.text }, `${qid}: ${other ?? labels.join(", ")}`);
      }
      case "note": if (!pos[0]) return usage('note "text" [--rule "! ..."]'); log(root, { ev: "note", text: pos[0], rule: one.rule ?? null }); return ok({ text: pos[0] }, `noted: ${pos[0]}`);
      case "notes": return ok(pr.notes, pr.notes.map((n) => `- ${n.text}${n.rule ? `  (${n.rule})` : ""}`).join("\n") || "no notes");
      case "save": {
        const { version: v, fresh } = save(root, { object: one.object, message: one.message, render: one.render, check: one.check, full: on.has("full"), task: one.task });
        return ok({ ...v, files: Object.keys(v.files).length, fresh }, `${fresh ? "saved" : "the same as"} v${v.n} of ${v.object} "${v.message}", ${checkLine(v.check)}`);
      }
      case "versions": {
        const o = objectOf(pr, one.object);
        const vs = versionsOf(o.dir);
        return ok(vs.map(({ files, ...v }) => ({ ...v, files: Object.keys(files).length })), vs.map((v) => `v${v.n} ${ago(v.at)} "${v.message}"${v.render ? " (render kept)" : ""}, ${checkLine(v.check)}`).join("\n") || `no versions of ${o.name} yet`);
      }
      case "diff": {
        const d = diff(objectOf(pr, one.object).dir, num(pos[0]), num(pos[1]));
        return ok(d, [...d.added.map((p) => `+ ${p}`), ...d.removed.map((p) => `- ${p}`), ...d.changed.map((p) => `~ ${p}`), d.render ? "~ the render" : ""].filter(Boolean).join("\n") || "the same");
      }
      case "checkout": {
        const o = objectOf(pr, one.object), n = num(pos[0]), into = pos[1];
        if (!into) return usage("checkout N DIR");
        writeVersion(root, o, n, into);
        return ok({ dir: into, version: n, object: o.name, render: versionN(o.dir, n).render?.path ?? null }, `v${n} of ${o.title} is in ${into}`);
      }
      case "restore": {
        const o = objectOf(pr, one.object), n = num(pos[0]);
        if (on.has("paths")) { const w = plan(o.dir, n); return ok(w, Object.keys(w).join("\n")); }
        const written = writeVersion(root, o, n);
        return ok({ version: n, object: o.name, written }, written.length ? `${o.title} is v${n} again (${written.length} file${written.length > 1 ? "s" : ""}: ${written.slice(0, 6).join(", ")}${written.length > 6 ? ", ..." : ""})` : `${o.title} is already v${n}`);
      }
      default: return usage(`no command ${cmd}`);
    }
  } catch (e) {
    return { out: { error: String((e as Error).message ?? e) }, text: String((e as Error).message ?? e), code: 1 };
  }
}

if (import.meta.main) {
  const a = process.argv.slice(2);
  const r = run(a.filter((x) => x !== "--json"));
  const s = a.includes("--json") ? JSON.stringify(r.out) : r.text;
  (r.code === 2 ? process.stderr : process.stdout).write(s + "\n");
  process.exit(r.code);
}
