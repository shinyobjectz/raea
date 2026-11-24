// Dan Harmon's story circle (his writing on Channel 101 and after), in our words: a character in a zone of comfort
// wants something, goes into an unfamiliar situation, adapts, gets what they wanted, pays a heavy price for it, returns
// to the familiar, and has changed. raea checks the price (a fall after the getting) and the change (the need or the
// lie settled, and the story not ending where it began).
import { finding, lens } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { at, beatsWith } from "./common.ts";

export const harmon = lens({
  name: "harmon",
  about: "You, need, go, search, find, take, return, change: a circle that comes home changed.",
  stages: ["you", "need", "go", "search", "find", "take", "return", "change"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const inc = s.firstShowing(s.inciting);
    const go = beatsWith(s, s.hero).find((b) => b.index > at(s, inc) && b.linked)?.key;
    const wantMet = s.events.find((x) => s.hero && x.ev.ends?.[s.hero] === "want")?.key;
    const peak = s.beats.filter((b) => b.index > at(s, go)).reduce<string | undefined>((hi, b) => (hi === undefined || s.focusLevel(b.key) > s.focusLevel(hi) ? b.key : hi), undefined);
    const find = s.firstShowing(wantMet) ?? peak;
    const take = s.beats.find((b) => b.index > at(s, find) && s.focusTurn(b.turns) < 0)?.key;
    const last = s.beats.at(-1)?.key;
    const map = {
      you: s.between(undefined, inc), need: inc ? [inc] : [], go: go ? [go] : [], search: s.between(go, find),
      find: find ? [find] : [], take: take ? [take] : [], return: take ? s.between(take, last) : [], change: last && last !== find && last !== take ? [last] : [],
    };
    if (!go) f.push(finding("harmon:go", "warn", "story", "the hero never goes anywhere new: no step out of the comfort zone after the change", "show the hero acting in an unfamiliar situation"));
    if (find && !take) f.push(finding("harmon:take", "warn", find, "they get what they wanted at no cost", "make the getting cost something: a turn down after it"));
    const settled = s.events.some((x) => s.hero && (x.ev.ends?.[s.hero] === "need" || x.ev.ends?.[s.hero] === "lie"));
    if (!settled) f.push(finding("harmon:change", "warn", s.hero ?? "story", "the circle closes but the hero hasn't changed: no need met or lie dropped", "end an event with the hero's need or lie (ends)"));
    if (last && s.focusLevel(last) === 0 && s.beats.length > 2) f.push(finding("harmon:change", "note", last, "the story ends at the level it began", "let the return land somewhere different"));
    return { map, findings: f, proposals: [] };
  },
});
