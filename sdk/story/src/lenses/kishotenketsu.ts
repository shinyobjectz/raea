// Kishōtenketsu (the four-part form of Chinese and Japanese poetry and of yonkoma comics), in our words: ki introduces,
// shō develops, ten turns (something new that makes the rest read differently), ketsu brings them together. It needs no
// conflict, so this lens never asks for a crisis or a villain; it asks for the turn, and for it to come late.
import { finding, lens } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { at } from "./common.ts";

export const kishotenketsu = lens({
  name: "kishotenketsu",
  about: "Introduction, development, a turn that makes the rest read differently, a conclusion that joins them. No conflict needed.",
  stages: ["ki", "sho", "ten", "ketsu"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const n = s.beats.length;
    const turnish = (b: (typeof s.beats)[number]) => b.beat.surprise === "meant" || (b.beat.reveals?.length ?? 0) > 0 || Object.values(b.turns).some((d) => Math.abs(d) >= 2);
    const ten = s.beats.filter((b) => b.index >= Math.floor(n / 2) && b.index < n).find(turnish)?.key ?? s.beats.find(turnish)?.key;
    const t = at(s, ten);
    const map = {
      ki: s.beats.filter((b) => b.index < Math.max(1, Math.floor((t < 0 ? n : t) / 2))).map((b) => b.key),
      sho: s.beats.filter((b) => b.index >= Math.max(1, Math.floor((t < 0 ? n : t) / 2)) && b.index < (t < 0 ? n : t)).map((b) => b.key),
      ten: ten ? [ten] : [], ketsu: ten ? s.beats.filter((b) => b.index > t).map((b) => b.key) : [],
    };
    if (!ten) f.push(finding("kishotenketsu:ten", "error", "story", "no ten: nothing new arrives to make the rest read differently", "add a late beat that reveals a fact or turns hard, or mark one surprise: \"meant\""));
    else if (t < Math.floor(n / 2)) f.push(finding("kishotenketsu:ten", "warn", ten, "the turn comes in the first half, before there's anything for it to change", "develop first; turn in the third quarter"));
    if (ten && !map.ketsu.length) f.push(finding("kishotenketsu:ketsu", "warn", ten, "the story stops on the turn: no ketsu brings the parts together", "add a closing beat that shows the first parts in the turn's light"));
    return { map, findings: f, proposals: [] };
  },
});
