// What the corpus rows (rows.ts) and a held-out story's rows (validate.ts) share: a telling as prose, the story as the
// extractor answers it, and exact labels for a telling a viewer could see.
import { checkStory, fromData, scriptOf, toLines, type AnyStory } from "../src/index.ts";

type J = Record<string, any>;

/** A story's split by its index: one in ten test, one in ten val, the rest train. */
export const split = (i: number) => { const h = (Math.imul(i + 1, 2654435761) >>> 0) % 10; return h === 0 ? "test" : h === 1 ? "val" : "train"; };

export const name = (k: string) => (k === "narrator" ? "NARRATOR" : k.replace(/([a-z])([A-Z])/g, "$1 $2").toUpperCase());

/** A telling as a script a person would write: numbered beats, what's seen, who says what. No keys, no annotations. */
export const prose = (beats: J[]): string => scriptOf(beats);

export const withTelling = (s: J, beats: J[]): J => ({ ...s, tellings: { main: beats } });
export const breaks = (s: J, beats: J[]) => {
  const st = fromData(withTelling(s, beats)).story as AnyStory;
  const keys = beats.map((b, i) => b.id ?? `b${i}`);
  const firstShowing = (ev: string) => beats.findIndex((b) => (b.shows ?? []).includes(ev));
  return checkStory(st, "main")
    .filter((f) => f.level !== "note")
    .map((f) => {
      let beat: number | null = keys.indexOf(f.about);
      if (beat < 0) beat = s.world?.[f.about] ? firstShowing(f.about) : -1;
      return { beat: beat >= 0 ? beat + 1 : null, check: f.check, level: f.level };
    })
    .sort((a, b) => (a.beat ?? 0) - (b.beat ?? 0) || a.check.localeCompare(b.check));
};

/** The story as the extractor answers it: everything but the words on screen. */
export const bare = (s: J): J => ({
  ...(s.title ? { title: s.title } : {}),
  ...(s.idea ? { idea: s.idea } : {}),
  values: s.values, facts: s.facts ?? {}, agents: s.agents, world: s.world,
  tellings: { main: s.tellings.main.map(({ onScreen: _o, says: _s, id: _i, ...b }: J) => b) },
});

export const CHECKS = ["turns", "and-then", "surprise", "cause", "dead-end", "setup", "belief", "rhythm", "arc", "claim", "idea"];
export const criticSchema = {
  type: "object", additionalProperties: false, required: ["breaks"],
  properties: { breaks: { type: "array", items: { type: "object", additionalProperties: false, required: ["beat", "check", "level"], properties: { beat: { anyOf: [{ type: "integer", minimum: 1 }, { type: "null" }] }, check: { type: "string", enum: CHECKS }, level: { type: "string", enum: ["error", "warn"] } } } } },
};

// ---- the windowed extractor (docs/story.md, "Any length") ----
// A script read a chunk of beats at a time. Each step is handed what earlier steps found (KNOWN, as story lines: the
// values, facts, agents and events so far) and this chunk's prose, and answers only what's new (keys first met here)
// and this chunk's beats. A chained run merges the steps into one story; nothing a step reads grows with the telling
// but KNOWN, which grows with the world.
export const CHUNK = 6;

/** Which chunk introduces each key: an event where it's first shown (a cause no later than its effect; one never shown
 *  with the next shown event after it in story order), an agent or fact with the first event or beat to use it. */
export function introductions(s: J, W = CHUNK): { events: Record<string, number>; agents: Record<string, number>; facts: Record<string, number>; values: Record<string, number>; n: number } {
  const beats: J[] = s.tellings.main;
  const n = Math.max(1, Math.ceil(beats.length / W));
  const keys = Object.keys(s.world);
  const ev: Record<string, number> = {};
  beats.forEach((b, i) => { for (const e of [].concat(b.shows ?? [])) ev[e] ??= Math.floor(i / W); });
  for (let i = keys.length - 1; i >= 0; i--) if (ev[keys[i]] === undefined) ev[keys[i]] = keys.slice(i + 1).map((k) => ev[k]).find((c) => c !== undefined) ?? n - 1;
  for (let pass = 0; pass < keys.length; pass++) {
    let moved = false;
    for (const k of keys) for (const c of [].concat(s.world[k].because ?? [])) if (ev[c] !== undefined && ev[c] > ev[k]) { ev[c] = ev[k]; moved = true; }
    if (!moved) break;
  }
  const ag: Record<string, number> = {}, fa: Record<string, number> = {}, va: Record<string, number> = {};
  const at = (m: Record<string, number>, k: string, c: number) => { if (m[k] === undefined || c < m[k]) m[k] = c; };
  for (const k of keys) {
    const e = s.world[k], c = ev[k];
    for (const a of [e.who, ...[].concat(e.with ?? []), ...Object.keys(e.ends ?? {})]) at(ag, a, c);
    for (const f of ["needs", "sets", "unsets", "learns", "proof"]) for (const x of [].concat(e[f] ?? [])) at(fa, x, c);
    for (const v of Object.keys(e.turns ?? {})) at(va, v, c);
  }
  beats.forEach((b, i) => {
    const c = Math.floor(i / W);
    for (const f of ["reveals", "hides", "asks"]) for (const x of [].concat(b[f] ?? [])) at(fa, x, c);
    for (const l of b.says ?? []) if (l.who !== "narrator") at(ag, l.who, c);
  });
  for (const [a, x] of Object.entries(s.agents as J)) { at(ag, a, n - 1); for (const f of [].concat(x.believes ?? [])) at(fa, f, ag[a]); }
  // a fact or value nothing uses, and the idea's value (the film is about it from the start), come in the first chunk
  for (const f of Object.keys(s.facts ?? {})) fa[f] ??= 0;
  for (const v of Object.keys(s.values)) va[v] ??= 0;
  if (s.idea?.value) va[s.idea.value] = 0;
  return { events: ev, agents: ag, facts: fa, values: va, n };
}

const pick = (o: J, ks: string[]) => Object.fromEntries(Object.keys(o ?? {}).filter((k) => ks.includes(k)).map((k) => [k, o[k]]));

/** One chunk's step: what's known before it (as data), its prose, and its answer. */
export function chunkSteps(s: J, W = CHUNK): { known: J; prompt: string; answer: J; from: number }[] {
  const intro = introductions(s, W);
  const beats: J[] = s.tellings.main;
  const b = bare(s);
  const sel = (m: Record<string, number>, test: (c: number) => boolean) => Object.keys(m).filter((k) => test(m[k]));
  return Array.from({ length: intro.n }, (_, j) => {
    const before = (c: number) => c < j, here = (c: number) => c === j;
    const known = { values: pick(b.values, sel(intro.values, before)), facts: pick(b.facts, sel(intro.facts, before)), agents: pick(b.agents, sel(intro.agents, before)), world: pick(b.world, sel(intro.events, before)) };
    const answer: J = {
      ...(j === 0 && b.title ? { title: b.title } : {}),
      ...(j === 0 && b.idea ? { idea: b.idea } : {}),
      values: pick(b.values, sel(intro.values, here)), facts: pick(b.facts, sel(intro.facts, here)),
      agents: pick(b.agents, sel(intro.agents, here)), world: pick(b.world, sel(intro.events, here)),
      beats: b.tellings.main.slice(j * W, (j + 1) * W),
    };
    const chunk = beats.slice(j * W, (j + 1) * W);
    const script = chunk.map((x, n) => prose([x]).replace(/^1\./, `${j * W + n + 1}.`)).join("\n");
    return { known, prompt: "", answer, from: j * W, script };
  }).map((st: J) => ({ ...st, prompt: knownPrompt(st.known, st.script) }));
}

/** The text a step reads: KNOWN as story lines, then the chunk's script. */
export function knownPrompt(known: J, script: string): string {
  const lines = Object.keys(known.values).length || Object.keys(known.agents).length || Object.keys(known.world).length
    ? toLines({ ...known, tellings: {} }).split("\n").filter((l) => !l.startsWith("TELLING")).join("\n")
    : "(nothing yet: this is the start)";
  return `KNOWN\n${lines}\nSCRIPT\n${script}`;
}

/** Steps merged into one story (a later step's key never overwrites an earlier one's). */
export function mergeSteps(answers: J[]): J {
  const out: J = { values: {}, facts: {}, agents: {}, world: {}, tellings: { main: [] } };
  for (const a of answers) {
    if (!a || typeof a !== "object") continue;
    if (a.title && !out.title) out.title = a.title;
    if (a.idea && !out.idea) out.idea = a.idea;
    for (const f of ["values", "facts", "agents", "world"]) for (const [k, v] of Object.entries((a[f] ?? {}) as J)) if (!(k in out[f])) out[f][k] = v;
    out.tellings.main.push(...(Array.isArray(a.beats) ? a.beats : []));
  }
  return out;
}

/** The step's answer shape: new keys and the chunk's beats. */
export function stepSchema(story: J): J {
  const p = story.properties;
  return { type: "object", additionalProperties: false, required: ["values", "facts", "agents", "world", "beats"], properties: { title: p.title, idea: p.idea, values: p.values, facts: p.facts, agents: p.agents, world: p.world, beats: p.tellings.additionalProperties } };
}
