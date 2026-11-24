// Exact checks (docs/story.feature ST2a): read only what the systems derived, never prose, and need no model. Each finding
// names the beat (b0, b1 ...) or event it's about, says what's wrong in a sentence and how to fix it.
import type { World } from "./ecs.ts";
import { beatsOf, eventsOf, infoOf, keysOf, load, type RAgent } from "./load.ts";
import { listOf, type AnyStory } from "./model.ts";
import { causes, type Causes } from "./systems/cause.ts";
import { knowledge, type Audience, type Break } from "./systems/know.ts";
import { tension } from "./systems/tension.ts";
import { values, type Curve } from "./systems/values.ts";

export type Level = "error" | "warn" | "note";
export type Finding = { check: string; level: Level; about: string; says: string; fix?: string };

/** Every system, in the order they need each other. */
export const derive = (w: World): World => w.run(values, knowledge, causes, tension);

/** A story told one way, loaded and derived: what lenses, checks, compilers and models read. */
export const analyze = (s: AnyStory, telling?: string, seed = 0): World => derive(load(s, telling, seed));

const RUN = 4; // beats in a row all turning one way before it's worth a note

/** Findings left once every beat's `meant` is taken out: a check a beat says it means, about that beat. */
export function unmeant(w: World, found: Finding[]): Finding[] {
  const meant = new Map(beatsOf(w).map(({ key, beat }) => [key, new Set(listOf(beat.meant))]));
  return found.filter((f) => !meant.get(f.about)?.has(f.check));
}

export function checkWorld(w: World): Finding[] {
  const out: Finding[] = [];
  const add = (check: string, level: Level, about: string, says: string, fix?: string) => out.push({ check, level, about, says, fix });
  const beats = beatsOf(w);
  const events = eventsOf(w);
  const c = w.resource<Causes>("Causes")!;

  for (const { e, key, beat } of beats) {
    const turns = w.must<Record<string, number>>(e, "Turns");
    const continues = w.get<string[]>(e, "Continues") ?? [];
    if (beat.shows.length && continues.length < beat.shows.length && !Object.values(turns).some((d) => d !== 0))
      add("turns", "warn", key, `${key} shows ${beat.shows.join(", ")} but turns no value: nothing changes`, "give a shown event a turn, or cut the beat");
    if (!w.must<boolean>(e, "Linked"))
      add("and-then", "warn", key, `${key} isn't caused by anything the audience has seen: an "and then", not a "therefore" or a "but"`, "show its cause earlier, or name a cause the audience saw");
    const a = w.must<Audience>(e, "Audience");
    if (a.surprise) add("surprise", "warn", key, `${key} turns hard on something nothing set up`, "set up a cause earlier, pay a setup here, or mark the beat surprise: \"meant\"");
  }
  for (const k of c.uncaused) add("cause", "error", k, `the event ${k} has no cause`, "name the earlier event that causes it (because), or mark it routine");
  for (const { event, cause } of c.later) add("cause", "error", event, `${event} is caused by ${cause}, which happens later in the world`, "reorder the world, or change the cause");
  for (const k of c.deadEnds) add("dead-end", "note", k, `the event ${k} causes nothing that follows`, "let a later event come from it, or cut it");

  const keyAt = (i: number) => beats[i]?.key ?? `b${i}`;
  const planted = new Map<string, number>();
  const paid = new Set<string>();
  for (const { key, beat, index } of beats) {
    for (const p of listOf(beat.pays)) {
      const at = planted.get(p);
      if (at === undefined) add("setup", "error", key, `${key} pays off "${p}", which nothing set up before it`, "plant it in an earlier beat (setup)");
      else if (at === index) add("setup", "warn", key, `${key} sets up and pays off "${p}" in one beat`, "plant it earlier");
      paid.add(p);
    }
    for (const s of listOf(beat.setup)) if (!planted.has(s)) planted.set(s, index);
  }
  for (const [s, at] of planted) if (!paid.has(s)) add("setup", "warn", keyAt(at), `${keyAt(at)} sets up "${s}", which is never paid off`, "pay it off in a later beat (pays), or cut the setup");

  for (const b of w.resource<Break[]>("BeliefBreaks") ?? [])
    add("belief", "error", b.event, `${b.who} acts on "${b.fact}" in ${b.event} without knowing it yet`, `let ${b.who} learn it earlier (an event that sets or learns it), or drop it from needs`);

  let run = 0, sign = 0;
  beats.forEach(({ e, key, beat }) => {
    // a shot that only continues the event before it is part of that beat's turn, not a new one
    if (beat.shows.length && (w.get<string[]>(e, "Continues") ?? []).length === beat.shows.length) return;
    const net = Object.values(w.must<Record<string, number>>(e, "Turns")).reduce((x, y) => x + y, 0);
    const s = Math.sign(net);
    run = s !== 0 && s === sign ? run + 1 : s !== 0 ? 1 : 0;
    sign = s || sign;
    if (run === RUN) add("rhythm", "note", key, `${RUN} beats in a row all turn ${s > 0 ? "up" : "down"}, ending at ${key}`, "break the run with a turn the other way");
  });

  const ids = new Map<string, number>();
  for (const { beat } of beats) if (beat.id !== undefined) ids.set(beat.id, (ids.get(beat.id) ?? 0) + 1);
  for (const [id, n] of ids) if (n > 1) add("id", "error", id, `${n} beats share the id "${id}", so a composition can't name its Sequence`, "give each beat its own id");
  const shown = new Set(beats.flatMap((b) => b.beat.shows));
  if (beats.length && !shown.size) add("turns", "warn", "story", "no beat shows an event: the telling is all narration", "show the events the beats are about");
  for (const [a, ent] of Object.entries(keysOf(w).agents)) {
    const ag = w.must<RAgent>(ent, "Agent");
    if (!ag.want && !ag.need) continue;
    const settled = events.some((x) => shown.has(x.key) && x.ev.ends?.[a] !== undefined);
    if (!settled) add("arc", "warn", a, `${a}'s ${ag.need ? "need" : "want"} is never settled in this telling`, `show an event that ends it (ends: { ${a}: "need" })`);
  }

  const set = new Set<string>();
  for (const x of events.sort((p, q) => p.order - q.order)) {
    for (const f of x.ev.sets ?? []) set.add(f);
    if (x.ev.claim !== undefined) {
      const proof = x.ev.proof ?? [];
      if (!proof.length) add("claim", "error", x.key, `${x.key} claims "${x.ev.claim}" with no fact behind it`, "name the facts that prove it (proof), set by events the story shows");
      for (const f of proof) if (!set.has(f)) add("claim", "error", x.key, `${x.key}'s claim rests on "${f}", which nothing in the world makes true by then`, "show the event that makes it true first");
    }
    for (const f of x.ev.unsets ?? []) set.delete(f);
  }

  const focus = infoOf(w).idea?.value;
  const curves = w.resource<Record<string, Curve>>("Curves") ?? {};
  if (focus && curves[focus]?.shape === "flat") add("idea", "warn", focus, `the story is about ${focus}, and ${focus} never moves`, "turn it in the beats that matter");
  return unmeant(w, out);
}

/** Check a story told one way: its findings, errors first. */
export function checkStory(s: AnyStory, telling?: string, seed = 0): Finding[] {
  const rank = { error: 0, warn: 1, note: 2 };
  return checkWorld(analyze(s, telling, seed)).sort((a, b) => rank[a.level] - rank[b.level]);
}
