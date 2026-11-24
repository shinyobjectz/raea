// A story as plain JSON, for anything outside TypeScript: a model that writes or reads stories, a file, the wire
// (docs/story.feature ST6a). fromData() reads JSON into a story and says what's wrong with it: a reference to a key that
// isn't declared, a field of the wrong shape. tsc checks a story written in code; this checks one that wasn't.
//
// The schemas are where the story models are schema-native: storySchema() is the shape of a whole story, and
// beatSchema(story) is the shape of one more beat *for that story*, its references enums of the story's own keys, so a
// generator decoding under it can't name an event, a fact or a setup the story doesn't have.
import type { AnyStory } from "./model.ts";
import { resolved, type RStory } from "./load.ts";

type Json = Record<string, unknown>;
export type DataError = { at: string; says: string };

/** A story as plain JSON: every choice point picked (with the seed), `kind` dropped. */
export function toData(s: AnyStory, seed = 0): Json {
  const { kind: _kind, ...rest } = resolved(s, seed) as RStory & { kind?: string };
  return JSON.parse(JSON.stringify(rest));
}

const str = { type: "string" };
const strs = { type: "array", items: str };
const enumOf = (keys: string[]) => (keys.length ? { type: "string", enum: keys } : str);
const listOf = (s: Json) => ({ type: "array", items: s });
const oneOrMany = (s: Json) => ({ anyOf: [s, listOf(s)] });

const MASKS = ["hero", "mentor", "ally", "herald", "threshold", "shapeshifter", "shadow", "trickster", "gift"];

/** One beat's shape; with keys, its references are enums of them. */
export function beatShape(k: { events?: string[]; facts?: string[]; agents?: string[]; setups?: string[] } = {}): Json {
  const ev = enumOf(k.events ?? []), fact = enumOf(k.facts ?? []), who = k.agents?.length ? { type: "string", enum: [...k.agents, "narrator"] } : str;
  return {
    type: "object",
    additionalProperties: false,
    required: ["shows"],
    properties: {
      id: str,
      shows: listOf(ev),
      reveals: listOf(fact),
      hides: listOf(fact),
      asks: listOf(fact),
      feel: str,
      setup: oneOrMany(str),
      pays: oneOrMany(k.setups?.length ? enumOf(k.setups) : str),
      stage: { type: "object", additionalProperties: str },
      surprise: { type: "string", enum: ["meant"] },
      meant: oneOrMany(str),
      says: listOf({ type: "object", additionalProperties: false, required: ["who", "says"], properties: { who, says: str, means: str, how: str } }),
      onScreen: str,
      seconds: { type: "number", minimum: 0 },
      scene: str,
    },
  };
}

/** The keys a story declares, by kind (setups are the ones its beats plant). */
export function keysIn(s: Json): { values: string[]; facts: string[]; agents: string[]; events: string[]; setups: string[] } {
  const keys = (o: unknown) => (o && typeof o === "object" ? Object.keys(o) : []);
  const tellings = Object.values((s.tellings ?? {}) as Record<string, Json[]>);
  const setups = [...new Set(tellings.flat().flatMap((b) => (b?.setup === undefined ? [] : ([] as unknown[]).concat(b.setup))).map(String))];
  return { values: keys(s.values), facts: keys(s.facts), agents: keys(s.agents), events: keys(s.world), setups };
}

/** The shape of one more beat for this story: what a generator decodes under. */
export const beatSchema = (s: AnyStory | Json, seed = 0): Json => {
  const k = keysIn((s as AnyStory).kind === "story" ? toData(s as AnyStory, seed) : (s as Json));
  return beatShape(k);
};

/** The shape of a whole story (keys free; fromData checks the references). */
export function storySchema(): Json {
  const turns = { type: "object", additionalProperties: { type: "integer", minimum: -3, maximum: 3 } };
  const event = {
    type: "object",
    additionalProperties: false,
    required: ["who", "does"],
    properties: {
      who: str, with: strs, does: str, because: oneOrMany(str), needs: strs, sets: strs, unsets: strs, learns: strs, turns,
      ends: { type: "object", additionalProperties: { type: "string", enum: ["want", "need", "lie", "want-refused"] } },
      dilemma: strs, claim: str, proof: strs, routine: { type: "boolean" },
    },
  };
  const agent = {
    type: "object",
    additionalProperties: false,
    properties: { want: str, need: str, lie: str, mask: { type: "string", enum: MASKS }, believes: strs, arc: { type: "string", enum: ["positive", "negative", "flat"] }, role: str },
  };
  return {
    type: "object",
    additionalProperties: false,
    required: ["values", "agents", "world", "tellings"],
    properties: {
      title: str,
      premise: str,
      idea: { type: "object", additionalProperties: false, required: ["side"], properties: { value: str, side: str, counter: str } },
      values: { type: "object", additionalProperties: { type: "object", additionalProperties: false, required: ["neg", "pos"], properties: { neg: str, pos: str } } },
      facts: { type: "object", additionalProperties: str },
      agents: { type: "object", additionalProperties: agent },
      world: { type: "object", additionalProperties: event },
      tellings: { type: "object", additionalProperties: listOf(beatShape()) },
      lenses: strs,
    },
  };
}

/**
 * Where a value doesn't fit a schema, as errors: the few JSON Schema words storySchema() uses (type, enum, anyOf,
 * properties, required, additionalProperties, items, minimum, maximum). The one schema that tells a model a story's
 * shape is the one that holds the story to it, so a field of the wrong type never reaches a reader.
 */
export function shapeErrors(x: unknown, schema: Json, at = ""): DataError[] {
  const out: DataError[] = [];
  const here = at || "the story";
  const kind = (v: unknown) => (v === null ? "null" : Array.isArray(v) ? "a list" : typeof v === "object" ? "an object" : typeof v === "string" ? "text" : typeof v);
  if (schema.anyOf) {
    const tries = (schema.anyOf as Json[]).map((s2) => shapeErrors(x, s2, at));
    return tries.some((t) => !t.length) ? [] : tries.sort((p, q) => p.length - q.length)[0];
  }
  const t = schema.type as string | undefined;
  const fits = t === undefined || (t === "object" ? !!x && typeof x === "object" && !Array.isArray(x) : t === "array" ? Array.isArray(x) : t === "integer" ? Number.isInteger(x) : t === "number" ? typeof x === "number" && Number.isFinite(x) : typeof x === t);
  if (!fits) return [{ at: here, says: `expected ${t === "array" ? "a list" : t === "object" ? "an object" : t === "string" ? "text" : t === "integer" ? "a whole number" : t === "boolean" ? "true or false" : `a ${t}`}, got ${kind(x)}${typeof x === "string" || typeof x === "number" ? ` (${JSON.stringify(x).slice(0, 30)})` : ""}` }];
  if (schema.enum && !(schema.enum as unknown[]).includes(x)) out.push({ at: here, says: `${JSON.stringify(x)} isn't one of ${(schema.enum as unknown[]).map((e) => JSON.stringify(e)).join(", ")}` });
  if (typeof x === "number") {
    if (typeof schema.minimum === "number" && x < schema.minimum) out.push({ at: here, says: `${x} is under ${schema.minimum}` });
    if (typeof schema.maximum === "number" && x > schema.maximum) out.push({ at: here, says: `${x} is over ${schema.maximum}` });
  }
  if (t === "array" && schema.items) (x as unknown[]).forEach((v, i) => out.push(...shapeErrors(v, schema.items as Json, `${at}[${i}]`)));
  if (t === "object") {
    const o = x as Json, props = (schema.properties ?? {}) as Record<string, Json>;
    for (const r of (schema.required ?? []) as string[]) if (o[r] === undefined) out.push({ at: here, says: `needs ${r}` });
    for (const [k, v] of Object.entries(o)) {
      if (v === undefined) continue;
      const sub = props[k] ?? (typeof schema.additionalProperties === "object" ? (schema.additionalProperties as Json) : undefined);
      const path = at ? `${at}.${k}` : k;
      if (sub) out.push(...shapeErrors(v, sub, path));
      else if (schema.additionalProperties === false) out.push({ at: path, says: `isn't a field here (${Object.keys(props).join(", ")})` });
    }
  }
  return out;
}

/** JSON read as a story, with everything wrong in it. A story with errors is still returned, for a critic to read. */
export function fromData(j: unknown): { story: AnyStory | null; errors: DataError[] } {
  const errors: DataError[] = [];
  const err = (at: string, says: string) => errors.push({ at, says });
  if (!j || typeof j !== "object" || Array.isArray(j)) return { story: null, errors: [{ at: "", says: "a story is a JSON object" }] };
  const s = j as Json;
  for (const f of ["values", "agents", "world", "tellings"]) if (!s[f] || typeof s[f] !== "object" || Array.isArray(s[f])) err(f, `a story needs ${f}, an object`);
  if (errors.length) return { story: null, errors };
  // the shape first: a field of the wrong type is refused here, before any reader trips on it
  errors.push(...shapeErrors(s, storySchema()));
  if (!Object.keys(s.tellings as Json).length) err("tellings", "a story needs at least one telling");
  if (errors.length) return { story: null, errors };
  const k = keysIn(s);
  const has = (kind: keyof typeof k, at: string, x: unknown) => {
    if (typeof x !== "string") return err(at, `expected a ${kind.slice(0, -1)} key, got ${JSON.stringify(x)}`);
    if (!k[kind].includes(x)) err(at, `"${x}" isn't one of the story's ${kind} (${k[kind].join(", ") || "none"})`);
  };
  const many = (kind: keyof typeof k, at: string, x: unknown) => ([] as unknown[]).concat(x ?? []).forEach((y, i) => has(kind, `${at}[${i}]`, y));
  for (const [a, ag] of Object.entries(s.agents as Record<string, Json>)) many("facts", `agents.${a}.believes`, ag?.believes);
  const order = k.events;
  for (const [e, ev] of Object.entries(s.world as Record<string, Json>)) {
    if (!ev || typeof ev !== "object") { err(`world.${e}`, "an event is an object"); continue; }
    has("agents", `world.${e}.who`, ev.who);
    if (typeof ev.does !== "string" || !ev.does) err(`world.${e}.does`, "an event says what happens (does)");
    many("agents", `world.${e}.with`, ev.with);
    many("events", `world.${e}.because`, ev.because);
    for (const c of ([] as unknown[]).concat(ev.because ?? [])) if (typeof c === "string" && order.indexOf(c) >= order.indexOf(e)) err(`world.${e}.because`, `"${c}" comes after ${e} in story order`);
    for (const f of ["needs", "sets", "unsets", "learns", "proof"]) many("facts", `world.${e}.${f}`, ev[f]);
    for (const [v, d] of Object.entries((ev.turns ?? {}) as Json)) {
      has("values", `world.${e}.turns`, v);
      if (typeof d !== "number" || !Number.isInteger(d) || Math.abs(d) > 3) err(`world.${e}.turns.${v}`, "a turn is a whole number of steps, -3 to 3");
    }
    for (const a of Object.keys((ev.ends ?? {}) as Json)) has("agents", `world.${e}.ends`, a);
  }
  for (const [t, beats] of Object.entries(s.tellings as Record<string, unknown>)) {
    if (!Array.isArray(beats)) { err(`tellings.${t}`, "a telling is a list of beats"); continue; }
    const ids = new Set<string>();
    beats.forEach((b: Json, i) => {
      const at = `tellings.${t}[${i}]`;
      if (!b || typeof b !== "object") return err(at, "a beat is an object");
      if (typeof b.id === "string") { if (ids.has(b.id)) err(`${at}.id`, `another beat in ${t} is already "${b.id}"`); ids.add(b.id); }
      if (b.seconds !== undefined && (typeof b.seconds !== "number" || !(b.seconds >= 0))) err(`${at}.seconds`, "seconds is a length, 0 or more");
      if (!Array.isArray(b.shows)) err(`${at}.shows`, "a beat lists the events it shows (shows: [] for none)");
      many("events", `${at}.shows`, b.shows);
      for (const f of ["reveals", "hides", "asks"]) many("facts", `${at}.${f}`, b[f]);
      many("setups", `${at}.pays`, b.pays);
      ([] as Json[]).concat((b.says as Json[]) ?? []).forEach((l, j) => {
        if (l?.who !== "narrator") has("agents", `${at}.says[${j}].who`, l?.who);
      });
    });
  }
  if (s.lenses !== undefined && (!Array.isArray(s.lenses) || s.lenses.some((l) => typeof l !== "string"))) err("lenses", "lenses are names (\"mckee\", \"pas\" ...)");
  if (s.idea && (s.idea as Json).value !== undefined) has("values", "idea.value", (s.idea as Json).value);
  return { story: { ...(s as object), kind: "story" } as AnyStory, errors };
}
