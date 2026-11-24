// Story lines (docs/story.md, "Any length"): a story as one keyword-led line per value, fact, agent, event and beat, the
// way Fountain writes a screenplay. The TypeScript data stays the story; this is a view of it that reads back exactly:
// what a person skims and edits, and what the story models read and write a line at a time (each line stands alone,
// so a model can stream, append or fix one, and a grammar can hold each line to its form).
//
//   TITLE Halden Outdoor: the first tent
//   VALUE nerve: afraid / bold
//   FACT pitched: the tent goes up in one pull
//   AGENT june: want a night outside | need to trust herself | role customer | mask hero
//   EVENT pitch: june with tent -- pulls the tent up in one go | because dusk | sets pitched | turns nerve+3 | ends june:need
//   TELLING 20s
//   BEAT hook: shows pitch | hides pitched | asks pitched | setup how | feel curiosity | on A tent glows on a dark hill.
//     SAYS june (quietly): Okay. // she's talking herself into it
//
// Fields are split on " | "; a text with "|" in it is written "\|". Choice points are picked first (with the seed).
import type { AnyStory } from "./model.ts";
import { toData, fromData, type DataError } from "./data.ts";

type J = Record<string, any>;
const esc = (s: unknown) => String(s).replace(/\|/g, "\\|").replace(/\n/g, " ");
const unesc = (s: string) => s.replace(/\\\|/g, "|");
const list = (x: unknown) => ([] as unknown[]).concat(x ?? []).map(String).join(",");
const unlist = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
const one = (xs: string[]) => (xs.length === 1 ? xs[0] : xs);
/** split on " | " not preceded by a backslash */
const fields = (s: string) => s.split(/(?<!\\) \| /).map((x) => unesc(x.trim()));

const AGENT_TEXT = ["want", "need", "lie"];
const EVENT_LISTS = ["because", "needs", "sets", "unsets", "learns", "proof"];
const BEAT_LISTS = ["shows", "reveals", "hides", "asks", "setup", "pays", "meant"];

/** A story as lines (its tellings all, or one). */
export function toLines(s: AnyStory | J, seed = 0): string {
  const d: J = (s as AnyStory).kind === "story" ? toData(s as AnyStory, seed) : s;
  const out: string[] = [];
  if (d.title) out.push(`TITLE ${esc(d.title)}`);
  if (d.premise) out.push(`PREMISE ${esc(d.premise)}`);
  if (d.idea) out.push(`IDEA ${d.idea.value ?? ""}: ${esc(d.idea.side)}${d.idea.counter ? ` / ${esc(d.idea.counter)}` : ""}`);
  if (d.lenses?.length) out.push(`LENSES ${list(d.lenses)}`);
  for (const [k, v] of Object.entries(d.values as J)) out.push(`VALUE ${k}: ${esc(v.neg)} / ${esc(v.pos)}`);
  for (const [k, v] of Object.entries((d.facts ?? {}) as J)) out.push(`FACT ${k}: ${esc(v)}`);
  for (const [k, a] of Object.entries(d.agents as J)) {
    const f: string[] = [];
    for (const t of AGENT_TEXT) if (a[t]) f.push(`${t} ${esc(a[t])}`);
    for (const t of ["mask", "role", "arc"]) if (a[t]) f.push(`${t} ${a[t]}`);
    if (a.believes?.length) f.push(`believes ${list(a.believes)}`);
    out.push(`AGENT ${k}${f.length ? `: ${f.join(" | ")}` : ""}`);
  }
  for (const [k, e] of Object.entries(d.world as J)) {
    const f = [`${e.who}${e.with?.length ? ` with ${list(e.with)}` : ""} -- ${esc(e.does)}`];
    for (const t of EVENT_LISTS) if (e[t] !== undefined && listOf(e[t]).length) f.push(`${t} ${list(e[t])}`);
    if (e.turns && Object.keys(e.turns).length) f.push(`turns ${Object.entries(e.turns).map(([v, n]) => `${v}${Number(n) >= 0 ? "+" : ""}${n}`).join(" ")}`);
    if (e.ends && Object.keys(e.ends).length) f.push(`ends ${Object.entries(e.ends).map(([a, x]) => `${a}:${x}`).join(" ")}`);
    if (e.dilemma?.length) f.push(`dilemma ${e.dilemma.map(esc).join(" / ")}`);
    if (e.claim) f.push(`claim ${esc(e.claim)}`);
    if (e.routine) f.push("routine");
    out.push(`EVENT ${k}: ${f.join(" | ")}`);
  }
  for (const [t, beats] of Object.entries(d.tellings as Record<string, J[]>)) {
    out.push(`TELLING ${t}`);
    beats.forEach((b) => {
      const f: string[] = [];
      for (const x of BEAT_LISTS) if (b[x] !== undefined && (x === "shows" || listOf(b[x]).length)) f.push(`${x} ${list(b[x])}`);
      if (b.feel) f.push(`feel ${esc(b.feel)}`);
      if (b.stage && Object.keys(b.stage).length) f.push(`stage ${Object.entries(b.stage).map(([l, st]) => `${l}:${st}`).join(" ")}`);
      if (b.surprise) f.push(`surprise ${b.surprise}`);
      if (b.scene) f.push(`scene ${esc(b.scene)}`);
      if (b.seconds !== undefined) f.push(`seconds ${b.seconds}`);
      if (b.onScreen) f.push(`on ${esc(b.onScreen)}`);
      out.push(`BEAT${b.id ? ` ${b.id}` : ""}: ${f.join(" | ")}`);
      // a line with no words says nothing to read back
      for (const l of (b.says ?? []).filter((x: J) => x?.says != null && x.says !== "")) out.push(`  SAYS ${l.who}${l.how ? ` (${esc(l.how)})` : ""}: ${esc(l.says)}${l.means ? ` // ${esc(l.means)}` : ""}`);
    });
  }
  return out.join("\n");
}
const listOf = (x: unknown) => ([] as unknown[]).concat(x ?? []);

const speaker = (k: string) => (k === "narrator" ? "NARRATOR" : k.replace(/([a-z])([A-Z])/g, "$1 $2").toUpperCase());
/** Beats as a script a person would write: numbered (from `from`), what's seen, who says what; no keys. */
export function scriptOf(beats: J[], from = 0): string {
  return beats.map((b, n) => [`${from + n + 1}. ${b.onScreen ?? ""}`.trimEnd(), ...(b.says ?? []).filter((l: J) => l?.says).map((l: J) => `   ${speaker(l.who)}: ${l.says}`)].join("\n")).join("\n");
}

/** Lines read back into a story, with everything wrong in them: a line that doesn't parse, then fromData's errors. */
export function fromLines(text: string): { story: AnyStory | null; errors: DataError[] } {
  const d: J = { values: {}, facts: {}, agents: {}, world: {}, tellings: {} };
  const errors: DataError[] = [];
  let telling: J[] | null = null;
  let beat: J | null = null;
  text.split("\n").forEach((raw, n) => {
    const line = raw.trimEnd();
    if (!line.trim()) return;
    const at = `line ${n + 1}`;
    const m = line.trim().match(/^([A-Z]+)\b ?(.*)$/);
    if (!m) return errors.push({ at, says: `a line starts with a keyword (VALUE, FACT, AGENT, EVENT, TELLING, BEAT, SAYS ...): ${line.slice(0, 60)}` });
    const [, kw, rest] = m;
    const kv = (s: string) => { const i = s.indexOf(": "); return i < 0 ? [s.replace(/:$/, "").trim(), ""] : [s.slice(0, i).trim(), s.slice(i + 2)]; };
    switch (kw) {
      case "TITLE": d.title = unesc(rest); break;
      case "PREMISE": d.premise = unesc(rest); break;
      case "LENSES": d.lenses = unlist(rest); break;
      case "IDEA": { const [v, r] = kv(rest); const [side, counter] = r.split(/(?<!\\) \/ /).map(unesc); d.idea = { ...(v ? { value: v } : {}), side, ...(counter ? { counter } : {}) }; break; }
      case "VALUE": { const [k, r] = kv(rest); const [neg, pos] = r.split(/(?<!\\) \/ /).map(unesc); d.values[k] = { neg, pos }; break; }
      case "FACT": { const [k, r] = kv(rest); d.facts[k] = unesc(r); break; }
      case "AGENT": {
        const [k, r] = kv(rest);
        const a: J = {};
        for (const f of r ? fields(r) : []) {
          const [t, ...v] = f.split(" "); const val = v.join(" ");
          if (t === "believes") a.believes = unlist(val); else a[t] = val;
        }
        d.agents[k] = a; break;
      }
      case "EVENT": {
        const [k, r] = kv(rest);
        const [head, ...fs] = fields(r);
        const hm = head.match(/^(\S+)(?: with (\S+))? -- (.*)$/);
        if (!hm) { errors.push({ at, says: `an event starts "who [with a,b] -- what happens": ${head.slice(0, 60)}` }); break; }
        const e: J = { who: hm[1], ...(hm[2] ? { with: unlist(hm[2]) } : {}), does: hm[3] };
        for (const f of fs) {
          const i = f.indexOf(" "); const t = i < 0 ? f : f.slice(0, i), v = i < 0 ? "" : f.slice(i + 1);
          if (EVENT_LISTS.includes(t)) e[t] = t === "because" ? one(unlist(v)) : unlist(v);
          else if (t === "turns") e.turns = Object.fromEntries(v.split(/\s+/).filter(Boolean).map((x) => { const mm = x.match(/^(.+?)([+-]\d+)$/); return mm ? [mm[1], Number(mm[2])] : [x, NaN]; }));
          else if (t === "ends") e.ends = Object.fromEntries(v.split(/\s+/).filter(Boolean).map((x) => x.split(":")));
          else if (t === "dilemma") e.dilemma = v.split(/(?<!\\) \/ /).map(unesc);
          else if (t === "claim") e.claim = v;
          else if (t === "routine") e.routine = true;
          else errors.push({ at, says: `an event has no field "${t}"` });
        }
        d.world[k] = e; break;
      }
      case "TELLING": telling = d.tellings[rest.trim()] = []; beat = null; break;
      case "BEAT": {
        if (!telling) { errors.push({ at, says: "a BEAT comes after a TELLING line" }); break; }
        const [id, r] = kv(rest);
        const b: J = id ? { id } : {};
        for (const f of r ? fields(r) : []) {
          const i = f.indexOf(" "); const t = i < 0 ? f : f.slice(0, i), v = i < 0 ? "" : f.slice(i + 1);
          if (t === "shows") b.shows = unlist(v);
          else if (t === "setup" || t === "pays" || t === "meant") b[t] = one(unlist(v));
          else if (BEAT_LISTS.includes(t)) b[t] = unlist(v);
          else if (t === "stage") b.stage = Object.fromEntries(v.split(/\s+/).filter(Boolean).map((x) => x.split(":")));
          else if (t === "seconds") b.seconds = Number(v);
          else if (t === "on") b.onScreen = v;
          else if (["feel", "surprise", "scene"].includes(t)) b[t] = v;
          else errors.push({ at, says: `a beat has no field "${t}"` });
        }
        b.shows ??= [];
        telling.push(b); beat = b; break;
      }
      case "SAYS": {
        if (!beat) { errors.push({ at, says: "SAYS comes under a BEAT" }); break; }
        const sm = rest.match(/^(\S+)(?: \((.*?)\))?: (.*?)(?: \/\/ (.*))?$/);
        if (!sm) { errors.push({ at, says: `a line is "SAYS who (how): words // what's meant"` }); break; }
        (beat.says ??= []).push({ who: sm[1], says: unesc(sm[3]), ...(sm[2] ? { how: unesc(sm[2]) } : {}), ...(sm[4] ? { means: unesc(sm[4]) } : {}) });
        break;
      }
      default: errors.push({ at, says: `no line starts with ${kw}` });
    }
  });
  if (!Object.keys(d.facts).length) delete d.facts;
  const r = fromData(d);
  return { story: r.story, errors: [...errors, ...r.errors] };
}
