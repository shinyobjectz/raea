// The cause graph (docs/story.feature ST1a). In the world: an event after the first with no cause (and not routine), a
// cause that happens after its effect, and an event that causes nothing and isn't the last ("leads nowhere"). In a
// telling: a beat none of whose events was caused by something the audience has already seen is an "and then" link
// (Parker and Stone's "therefore / but" rule; McKee's causality). Showing an event again doesn't explain it: a
// flash-forward's event shown a second time still needs a cause the audience has seen. Beats get "Linked"; the world
// gets "Causes".
import type { World } from "../ecs.ts";
import { beatsOf, eventsOf } from "../load.ts";
import { causesOf } from "../model.ts";

export type Causes = { uncaused: string[]; later: { event: string; cause: string }[]; deadEnds: string[]; edges: [string, string][] };

export function causes(w: World): void {
  const events = eventsOf(w).sort((a, b) => a.order - b.order);
  const order = Object.fromEntries(events.map((x) => [x.key, x.order]));
  const out: Causes = { uncaused: [], later: [], deadEnds: [], edges: [] };
  const causesSomething = new Set<string>();
  for (const { key, ev, order: o } of events) {
    const cs = causesOf(ev);
    if (o > 0 && cs.length === 0 && !ev.routine) out.uncaused.push(key);
    for (const c of cs) {
      out.edges.push([c, key]);
      causesSomething.add(c);
      if ((order[c] ?? -1) >= o) out.later.push({ event: key, cause: c });
    }
  }
  // the resolution needn't cause anything: events after the last one that settles a want or need (else the last event)
  const settles = events.filter((x) => Object.keys(x.ev.ends ?? {}).length).map((x) => order[x.key] ?? -1);
  const resolved = settles.length ? Math.max(...settles) : (order[events[events.length - 1]?.key] ?? 0) - 1;
  for (const { key, ev } of events) if ((order[key] ?? 0) <= resolved && !causesSomething.has(key) && !ev.routine && !Object.keys(ev.ends ?? {}).length) out.deadEnds.push(key);
  w.setResource<Causes>("Causes", out);

  const byKey = Object.fromEntries(events.map((x) => [x.key, x.ev]));
  const seen = new Set<string>();
  for (const { e, beat, index } of beatsOf(w)) {
    const linked =
      index === 0 ||
      beat.shows.length === 0 ||
      beat.shows.some((s) => {
        const ev = byKey[s];
        if (!ev) return false;
        if (ev.routine || order[s] === 0) return true;
        return causesOf(ev).some((c) => seen.has(c) || beat.shows.includes(c));
      });
    w.insert(e, "Linked", linked);
    for (const s of beat.shows) seen.add(s);
  }
}
