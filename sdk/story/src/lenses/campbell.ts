// Campbell's monomyth (The Hero with a Thousand Faces, 1949), in our words: a hero is called out of the known world,
// may refuse, is helped across a threshold, goes down through trials to an ordeal at the bottom, takes what they came
// for, and brings it back. raea reads it as a value curve: the ordeal is the lowest point after the threshold, and the
// boon must lift the story above it. The stage names follow the book's three movements, shortened.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { at, beatsWith, lowest, mentorOf } from "./common.ts";

export const campbell = lens({
  name: "campbell",
  about: "Called out of the known world, helped across a threshold, down through trials to an ordeal, back with a boon.",
  stages: ["call", "refusal", "aid", "threshold", "trials", "ordeal", "boon", "return"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const L = (st: string) => s.label("campbell", st);
    const call = L("call") ?? s.firstShowing(s.inciting);
    const mentor = mentorOf(s);
    const aid = L("aid") ?? beatsWith(s, mentor).find((b) => b.index >= at(s, call))?.key ?? beatsWith(s, mentor)[0]?.key;
    const heroActs = beatsWith(s, s.hero).filter((b) => b.index > Math.max(at(s, call), at(s, aid)));
    const threshold = L("threshold") ?? heroActs.find((b) => b.linked)?.key;
    const boon = L("boon") ?? s.lastShowing(s.climax);
    // the abyss: the lowest beat before the boon, from the threshold when it comes first, else from the call
    const from = threshold && at(s, threshold) < at(s, boon) ? threshold : call;
    const ordeal = L("ordeal") ?? lowest(s, (from ? [from] : []).concat(s.between(from, boon)));
    // a refusal: the hero turning down, or weighing a dilemma, between the call and the help
    const refusal = s.between(call, aid ?? threshold).filter((k) => { const b = s.beats.find((x) => x.key === k)!; return s.focusTurn(b.turns) < 0 || b.beat.shows.some((e) => (s.events.find((x) => x.key === e)?.ev.dilemma?.length ?? 0) >= 2); });
    const map = {
      call: call ? [call] : [], refusal, aid: aid ? [aid] : [], threshold: threshold ? [threshold] : [],
      trials: s.between(threshold, ordeal), ordeal: ordeal && ordeal !== threshold && ordeal !== call ? [ordeal] : [], boon: boon ? [boon] : [],
      return: boon ? s.beats.filter((b) => b.index > at(s, boon)).map((b) => b.key) : [],
    };
    if (!call) f.push(finding("campbell:call", "error", "story", "no call: nothing pulls the hero out of the known world", "give the hero an event that turns their world by 2 or more"));
    if (!mentor || !aid) {
      f.push(finding("campbell:aid", "warn", "story", "the hero crosses alone: no mentor or gift helps", "let a mentor or gift (mask) act with the hero before the threshold"));
      p.push({ stage: "aid", after: call, onScreen: `someone or something gives ${s.hero ?? "the hero"} what they need to go on`, feel: "hope", why: "the aid makes the crossing possible and names what's at stake" });
    }
    if (from === threshold && threshold && ordeal && s.focusLevel(ordeal) >= s.focusLevel(threshold) && s.between(threshold, boon).length)
      f.push(finding("campbell:ordeal", "warn", ordeal, "no descent: after the threshold the story never goes lower", "let the hero lose something on the way to the boon"));
    if (boon && ordeal && s.focusLevel(boon) <= s.focusLevel(ordeal)) f.push(finding("campbell:boon", "error", boon, "the hero brings nothing back: the boon doesn't lift the story above the ordeal", "turn the focus value up in the climax"));
    if (!map.return.length) f.push(finding("campbell:return", "note", "story", "no return: we never see the boon brought home", "end on the known world, changed"));
    return { map, findings: f, proposals: p };
  },
});
