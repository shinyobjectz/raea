// Propp's Morphology of the Folktale (1928), in our words: a tale is a sequence of functions (what a character does for
// the plot) carried by a few roles: villain, donor, helper, the sought-for person, dispatcher, hero, false hero. raea
// reads the spine every tale shares: a villainy or a lack, a departure, a donor who tests the hero before giving, a
// struggle, and the lack made good. Masks carry the roles: shadow the villain, mentor or gift the donor, herald the
// dispatcher, shapeshifter the false hero.
import { finding, lens } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { actsIn, at, beatsWith, masked, mentorOf } from "./common.ts";

export const propp = lens({
  name: "propp",
  about: "Villainy or lack, departure, the donor's test and gift, struggle, the lack made good, return and recognition.",
  stages: ["lack", "departure", "test", "gift", "struggle", "liquidation", "return", "recognition"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const lackEv = s.events.find((x) => s.focusTurn((x.ev.turns ?? {}) as Record<string, number>) < 0)?.key;
    const lack = s.firstShowing(lackEv);
    const donor = mentorOf(s), villain = masked(s, "shadow");
    const departure = beatsWith(s, s.hero).find((b) => b.index > at(s, lack) && b.linked)?.key;
    const donorBeats = beatsWith(s, donor);
    const gift = donorBeats.find((b) => b.index > at(s, lack))?.key;
    const test = gift ? s.beats.filter((b) => b.index < at(s, gift) && b.index > at(s, lack) && b.beat.shows.some((e) => s.events.find((x) => x.key === e)?.ev.dilemma?.length)).map((b) => b.key) : [];
    const struggle = villain ? beatsWith(s, villain).filter((b) => b.beat.shows.some((e) => actsIn(s, e, s.hero))).map((b) => b.key) : [];
    const liq = s.beats.find((b) => b.index > at(s, lack) && s.focusLevel(b.key) >= 0 && s.focusTurn(b.turns) > 0)?.key;
    const recog = s.beats.filter((b) => b.index > at(s, liq) && (b.beat.reveals?.length ?? 0) > 0).map((b) => b.key);
    const map = {
      lack: lack ? [lack] : [], departure: departure ? [departure] : [], test, gift: gift ? [gift] : [], struggle,
      liquidation: liq ? [liq] : [], return: liq ? s.beats.filter((b) => b.index > at(s, liq)).map((b) => b.key) : [], recognition: recog,
    };
    if (!lack) f.push(finding("propp:lack", "error", "story", "no villainy and no lack: nothing is taken or missing", "open with what the hero lacks or loses (a turn down)"));
    if (lack && !liq) f.push(finding("propp:liquidation", "error", lack, "the lack is never made good", "end with the focus value back at or above where it started"));
    if (gift && !test.length) f.push(finding("propp:test", "note", gift, "the donor gives without testing: the hero earns nothing", "put a dilemma before the gift"));
    if (villain && !struggle.length) f.push(finding("propp:struggle", "warn", villain, `${villain} is the villain but never meets the hero`, `show ${villain} and ${s.hero ?? "the hero"} in one event`));
    return { map, findings: f, proposals: [] };
  },
});
