// Vogler's Writer's Journey (1992, from his memo on Campbell for Disney), in our words: twelve stages in three acts, and
// the archetypes as masks a character wears for a beat, not types. raea checks the masks by what they do: a herald
// brings the call, a shadow acts against the hero, a mentor helps; and the journey's late test, the resurrection, as a
// second fall after the reward.
import { finding, lens } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { actsIn, at, beatsWith, lowest, masked, mentorOf } from "./common.ts";

export const vogler = lens({
  name: "vogler",
  about: "Twelve stages from the ordinary world to the return with the elixir; archetypes as masks that act.",
  stages: ["ordinary-world", "call", "refusal", "mentor", "threshold", "tests", "approach", "ordeal", "reward", "road-back", "resurrection", "return"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const L = (st: string) => s.label("vogler", st);
    const call = L("call") ?? s.firstShowing(s.inciting);
    const mentor = mentorOf(s);
    const meet = L("mentor") ?? beatsWith(s, mentor).find((b) => b.index >= at(s, call))?.key;
    const threshold = L("threshold") ?? beatsWith(s, s.hero).find((b) => b.index > Math.max(at(s, call), at(s, meet)) && b.linked)?.key;
    const climax = L("resurrection") ?? s.lastShowing(s.climax);
    const from = threshold ?? call;
    const ordeal = L("ordeal") ?? lowest(s, s.between(from, climax));
    const reward = L("reward") ?? s.beats.find((b) => b.index > at(s, ordeal) && b.index < at(s, climax) && s.focusTurn(b.turns) > 0)?.key;
    const map = {
      "ordinary-world": s.between(undefined, call), call: call ? [call] : [],
      refusal: s.between(call, meet ?? threshold).filter((k) => s.focusTurn(s.beats.find((b) => b.key === k)!.turns) < 0),
      mentor: meet ? [meet] : [], threshold: threshold ? [threshold] : [],
      tests: s.between(from, ordeal).slice(0, -1), approach: s.between(from, ordeal).slice(-1),
      ordeal: ordeal ? [ordeal] : [], reward: reward ? [reward] : [], "road-back": s.between(reward ?? ordeal, climax),
      resurrection: climax ? [climax] : [], return: climax ? s.beats.filter((b) => b.index > at(s, climax)).map((b) => b.key) : [],
    };
    if (!s.hero || !s.agents[s.hero]?.mask) f.push(finding("vogler:hero", "note", "story", "no one wears the hero's mask", "mask the protagonist hero"));
    const herald = masked(s, "herald");
    if (herald && s.inciting && !actsIn(s, s.inciting, herald)) f.push(finding("vogler:herald", "warn", herald, `${herald} wears the herald's mask but doesn't bring the call`, `let ${herald} act in ${s.inciting}`));
    const shadow = masked(s, "shadow");
    if (shadow && !s.events.some((x) => (x.ev.who === shadow || (x.ev.with ?? []).includes(shadow)) && s.focusTurn((x.ev.turns ?? {}) as Record<string, number>) < 0))
      f.push(finding("vogler:shadow", "warn", shadow, `${shadow} wears the shadow's mask but never acts against the hero`, `give ${shadow} an event that turns the focus value down`));
    if (!reward && ordeal) f.push(finding("vogler:reward", "warn", ordeal, "nothing is won after the ordeal before the climax", "show the hero seizing something after the low point"));
    if (reward && climax && !s.between(reward, climax).some((k) => s.focusTurn(s.beats.find((b) => b.key === k)!.turns) < 0) && s.focusTurn(s.beats.find((b) => b.key === climax)!.turns) <= 0)
      f.push(finding("vogler:resurrection", "note", climax, "no resurrection: the hero is never tested a last time", "let the road back cost something before the final turn"));
    return { map, findings: f, proposals: [] };
  },
});
