// A story and one of its tellings into a world (docs/story.feature ST0b). Choice points are resolved first, with the seed
// given (sample.ts), so a loaded world is always one definite story. Components:
//   values  { Key, Value }        facts  { Key, Fact }        agents  { Key, Agent }
//   events  { Key, Event, Order }  (Order: its place in the world's story order)
//   beats   { Key, Beat, Index }   (Index: its place in the telling; Key its id, else "b0", "b1" ...)
// Resources: "Story" (title, premise, idea, lenses, telling's name) and "Keys" (each key's entity, by kind).
import { World, type Entity } from "./ecs.ts";
import type { Agent, AnyStory, Beat, Event, Value } from "./model.ts";
import { resolve } from "./resolve.ts";

export type Resolved<T> = T extends { choice: readonly (infer U)[] } ? U : T extends object ? { [K in keyof T]: Resolved<T[K]> } : T;
export type REvent = Resolved<Event<string, string, string, string>>;
export type RBeat = Resolved<Beat<string, string, string, string>>;
export type RAgent = Agent<string>;
/** A story with every choice point picked: plain beats and events throughout. */
export type RStory = Omit<AnyStory, "tellings" | "world"> & { tellings: Record<string, RBeat[]>; world: Record<string, REvent> };
/** The story resolved for this seed, typed as plain data. */
export const resolved = (s: AnyStory, seed = 0): RStory => resolve(s, seed) as unknown as RStory;

export type Keys = { values: Record<string, Entity>; facts: Record<string, Entity>; agents: Record<string, Entity>; events: Record<string, Entity>; beats: Entity[] };
export type StoryInfo = { title?: string; premise?: string; idea?: AnyStory["idea"]; lenses: string[]; telling: string };

/** The world of a story told one way. `telling` defaults to the story's first. */
export function load(s: AnyStory, telling?: string, seed = 0): World {
  const r = resolved(s, seed);
  const name = telling ?? Object.keys(r.tellings)[0];
  const beats = r.tellings[name];
  if (!beats) throw new Error(`the story has no telling named ${name} (it has ${Object.keys(r.tellings).join(", ")})`);
  const w = new World();
  const keys: Keys = { values: {}, facts: {}, agents: {}, events: {}, beats: [] };
  for (const [k, v] of Object.entries(r.values as Record<string, Value>)) keys.values[k] = w.spawn({ Key: k, Value: v });
  for (const [k, f] of Object.entries((r.facts ?? {}) as Record<string, string>)) keys.facts[k] = w.spawn({ Key: k, Fact: { text: f } });
  for (const [k, a] of Object.entries(r.agents as Record<string, RAgent>)) keys.agents[k] = w.spawn({ Key: k, Agent: a });
  Object.entries(r.world as Record<string, REvent>).forEach(([k, e], i) => (keys.events[k] = w.spawn({ Key: k, Event: e, Order: i })));
  (beats as RBeat[]).forEach((b, i) => keys.beats.push(w.spawn({ Key: b.id ?? `b${i}`, Beat: b, Index: i })));
  w.setResource<Keys>("Keys", keys);
  w.setResource<StoryInfo>("Story", { title: r.title, premise: r.premise as string | undefined, idea: r.idea, lenses: r.lenses ?? [], telling: name });
  return w;
}

/** Helpers every system uses. */
export const keysOf = (w: World): Keys => w.resource<Keys>("Keys")!;
export const infoOf = (w: World): StoryInfo => w.resource<StoryInfo>("Story")!;
export const beatsOf = (w: World): { e: Entity; key: string; beat: RBeat; index: number }[] =>
  keysOf(w).beats.map((e) => ({ e, key: w.must<string>(e, "Key"), beat: w.must<RBeat>(e, "Beat"), index: w.must<number>(e, "Index") }));
export const eventsOf = (w: World): { e: Entity; key: string; ev: REvent; order: number }[] =>
  Object.entries(keysOf(w).events).map(([key, e]) => ({ e, key, ev: w.must<REvent>(e, "Event"), order: w.must<number>(e, "Order") }));
export const eventByKey = (w: World, key: string): REvent | undefined => {
  const e = keysOf(w).events[key];
  return e === undefined ? undefined : w.get<REvent>(e, "Event");
};
