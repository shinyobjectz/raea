// What several lenses read the same way: who's in an event, the beats an agent acts in, the lowest beat in a stretch.
import type { Structure } from "../lens.ts";

type B = Structure["beats"][number];
/** Whether an agent acts in an event (as who, or with). */
export const actsIn = (s: Structure, ev: string, agent: string | undefined) => {
  const e = s.events.find((x) => x.key === ev)?.ev;
  return !!agent && !!e && (e.who === agent || (e.with ?? []).includes(agent));
};
/** The beats where an agent acts. */
export const beatsWith = (s: Structure, agent: string | undefined): B[] => s.beats.filter((b) => b.beat.shows.some((e) => actsIn(s, e, agent)));
/** The agent wearing a mask (the first, when several do). */
export const masked = (s: Structure, mask: string) => Object.keys(s.agents).find((a) => s.agents[a].mask === mask);
/** The beat whose focus level is lowest among these keys (the earliest of equals). */
export const lowest = (s: Structure, keys: string[]) => keys.reduce<string | undefined>((lo, k) => (lo === undefined || s.focusLevel(k) < s.focusLevel(lo) ? k : lo), undefined);
/** A beat's place, or -1. */
export const at = (s: Structure, k: string | undefined) => s.beats.find((b) => b.key === k)?.index ?? -1;
/** The mentor: a mentor or gift mask, else the brand. */
export const mentorOf = (s: Structure) => masked(s, "mentor") ?? masked(s, "gift") ?? s.brand;
