// Blake Snyder's beat sheet (Save the Cat!, 2005), in our words: an opening image and a final image that mirror it, a
// catalyst early, a debate, a break into the new world, a midpoint that raises the stakes (a false win or a false
// loss), the bad guys closing in, an all-is-lost moment and a dark night, then a break into the finale. raea reads its
// positions as shares of the telling, so a 30-second ad and a feature are held to the same shape.
import { finding, lens } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { at, lowest } from "./common.ts";

export const saveTheCat = lens({
  name: "save-the-cat",
  about: "Opening and final images that mirror, a catalyst early, a midpoint that raises the stakes, all is lost, then the finale.",
  stages: ["opening-image", "catalyst", "debate", "break-into-two", "midpoint", "bad-guys-close-in", "all-is-lost", "finale", "final-image"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const n = s.beats.length;
    if (!n) return { map: {}, findings: f, proposals: [] };
    const share = (k: string | undefined) => (at(s, k) + 0.5) / n;
    const first = s.beats[0].key, last = s.beats[n - 1].key;
    const catalyst = s.firstShowing(s.inciting);
    const mid = s.beats.filter((b) => b.index > 0 && b.index < n - 1).sort((a, b) => Math.abs((a.index + 0.5) / n - 0.5) - Math.abs((b.index + 0.5) / n - 0.5))
      .find((b) => Object.values(b.turns).some((d) => Math.abs(d) >= 2))?.key;
    const finale = s.lastShowing(s.climax);
    const lost = lowest(s, s.between(catalyst, finale));
    const brk = s.beats.find((b) => b.index > at(s, catalyst) && b.linked && s.focusTurn(b.turns) !== 0)?.key;
    const map = {
      "opening-image": [first], catalyst: catalyst ? [catalyst] : [], debate: s.between(catalyst, brk), "break-into-two": brk ? [brk] : [],
      midpoint: mid ? [mid] : [], "bad-guys-close-in": s.between(mid, lost), "all-is-lost": lost ? [lost] : [], finale: finale ? [finale] : [], "final-image": [last],
    };
    if (catalyst && share(catalyst) > 0.34) f.push(finding("save-the-cat:catalyst", "warn", catalyst, `the catalyst comes ${Math.round(share(catalyst) * 100)}% of the way in`, "bring it into the first third"));
    if (n >= 5 && !mid) f.push(finding("save-the-cat:midpoint", "note", "story", "no midpoint: nothing near the middle raises the stakes", "give a middle beat a big turn, a false win or a false loss"));
    if (lost && share(lost) < 0.5 && n >= 4) f.push(finding("save-the-cat:all-is-lost", "warn", lost, "the lowest point comes in the first half: the story peaks downward too soon", "move the worst moment just before the finale"));
    if (s.focusLevel(last) === s.focusLevel(first) && n >= 3) f.push(finding("save-the-cat:final-image", "warn", last, "the final image is where the opening image was: nothing changed between them", "end on the opening world, changed"));
    return { map, findings: f, proposals: [] };
  },
});
