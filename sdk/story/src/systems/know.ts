// Knowledge (docs/story.feature ST1a): who knows what, when.
// In the world (story order): each agent starts with what they believe; an event's participants learn what it sets,
// its agent learns what it `learns`; an event whose agent acts on (`needs`) a fact they don't hold is a belief break,
// the plot hole FlawedFictions measures, found here from the data.
// In a telling (the audience's order): a beat teaches the audience what its events set, less what it `hides`, plus what
// it `reveals`; `asks` opens a question, and learning the fact closes it. On a beat:
//   suspense  facts the audience holds that an agent on screen doesn't and will need (or that the world sets later)
//   mystery   the questions still open
//   surprise  a big turn none of whose causes the audience has seen, and no setup paid (unless the beat means it)
import type { World } from "../ecs.ts";
import { beatsOf, eventsOf, keysOf, type RAgent, type REvent } from "../load.ts";
import { causesOf, listOf } from "../model.ts";

export type Break = { event: string; who: string; fact: string };
export type Audience = { knows: string[]; learned: string[]; open: string[]; answered: string[]; suspense: { who: string; fact: string }[]; surprise: boolean };

export function knowledge(w: World): void {
  const k = keysOf(w);
  const known: Record<string, Set<string>> = {};
  for (const [a, e] of Object.entries(k.agents)) known[a] = new Set(w.must<RAgent>(e, "Agent").believes ?? []);
  const breaks: Break[] = [];
  const events = eventsOf(w).sort((a, b) => a.order - b.order);
  const knowsBefore: Record<string, Record<string, string[]>> = {};
  for (const { e, key, ev } of events) {
    knowsBefore[key] = Object.fromEntries(Object.entries(known).map(([a, s]) => [a, [...s]]));
    for (const f of ev.needs ?? []) if (!known[ev.who]?.has(f)) breaks.push({ event: key, who: ev.who, fact: f });
    for (const who of [ev.who, ...(ev.with ?? [])]) for (const f of ev.sets ?? []) known[who]?.add(f);
    for (const f of ev.learns ?? []) known[ev.who]?.add(f);
    for (const who of [ev.who, ...(ev.with ?? [])]) for (const f of ev.unsets ?? []) known[who]?.delete(f);
    w.insert(e, "KnowsBefore", knowsBefore[key]);
  }
  w.setResource<Break[]>("BeliefBreaks", breaks);

  const byKey: Record<string, { order: number; ev: REvent }> = Object.fromEntries(events.map((x) => [x.key, { order: x.order, ev: x.ev }]));
  const audience = new Set<string>();
  const open = new Set<string>();
  const seen = new Set<string>();
  for (const { e, beat, index } of beatsOf(w)) {
    const learned = new Set<string>();
    for (const s of beat.shows) for (const f of byKey[s]?.ev.sets ?? []) if (!(beat.hides ?? []).includes(f)) learned.add(f);
    for (const f of beat.reveals ?? []) learned.add(f);
    for (const f of learned) audience.add(f);
    // an event the audience sees undo a fact takes it back (a lie exposed, an alibi broken), unless the beat hides that
    for (const s of beat.shows) for (const f of byKey[s]?.ev.unsets ?? []) if (!(beat.hides ?? []).includes(f)) audience.delete(f);
    const answered = [...open].filter((f) => audience.has(f));
    for (const f of answered) open.delete(f);
    for (const f of beat.asks ?? []) if (!audience.has(f)) open.add(f);

    // suspense: an agent on screen lacks a fact the audience holds, and needs it later or the world sets it later
    const suspense: { who: string; fact: string }[] = [];
    const first = [...beat.shows].sort((a, b) => (byKey[a]?.order ?? 0) - (byKey[b]?.order ?? 0))[0];
    const now = first !== undefined ? byKey[first]?.order ?? 0 : 0;
    const onScreen = new Set(beat.shows.flatMap((s) => (byKey[s] ? [byKey[s].ev.who, ...(byKey[s].ev.with ?? [])] : [])));
    for (const who of onScreen) {
      const theirs = new Set(first !== undefined ? knowsBefore[first]?.[who] ?? [] : []);
      for (const f of audience) {
        if (theirs.has(f)) continue;
        const later = events.some((x) => x.order >= now && ((x.ev.who === who && (x.ev.needs ?? []).includes(f)) || (x.order > now && (x.ev.sets ?? []).includes(f))));
        if (later) suspense.push({ who, fact: f });
      }
    }

    // surprise: a big turn whose causes the audience never saw, with no setup paid
    const big = beat.shows.some((s) => Object.values(byKey[s]?.ev.turns ?? {}).reduce<number>((a, d) => a + Math.abs(d ?? 0), 0) >= 2);
    const causesSeen = beat.shows.every((s) => {
      const cs = causesOf(byKey[s]?.ev ?? {});
      return cs.length === 0 || cs.some((c) => seen.has(c) || beat.shows.includes(c)) || seen.has(s);
    });
    const surprise = index > 0 && big && !causesSeen && listOf(beat.pays).length === 0 && beat.surprise !== "meant";
    for (const s of beat.shows) seen.add(s);

    w.insert<Audience>(e, "Audience", { knows: [...audience], learned: [...learned], open: [...open], answered, suspense, surprise });
  }
}
