// McKee's lens (Story, 1997): the five parts of a story's design (inciting incident, progressive complications, crisis,
// climax, resolution), each scene a turn of a value, the crisis a true dilemma, the climax answering what the inciting
// incident opened. In our words; the stages' names are the book's.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";

export const mckee = lens({
  name: "mckee",
  about: "Inciting incident, complications that escalate, a crisis that's a real dilemma, a climax that answers it, a resolution.",
  stages: ["inciting", "complications", "crisis", "climax", "resolution"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const at = (k: string | undefined) => s.beats.find((b) => b.key === k)?.index ?? -1;
    const L = (st: string) => s.label("mckee", st);
    const inc = L("inciting") ?? s.firstShowing(s.inciting), cri = L("crisis") ?? s.lastShowing(s.crisis), cli = L("climax") ?? s.lastShowing(s.climax);
    const map = {
      inciting: inc ? [inc] : [],
      complications: s.beats.filter((b) => b.index > at(inc) && b.index < (cri ? at(cri) : at(cli))).map((b) => b.key),
      crisis: cri ? [cri] : [],
      climax: cli ? [cli] : [],
      resolution: cli ? s.beats.filter((b) => b.index > at(cli)).map((b) => b.key) : [],
    };
    const n = s.beats.length;
    if (!inc) {
      f.push(finding("mckee:inciting", "error", "story", "nothing upsets the balance: no event turns a value by 2 or more", "give the protagonist's world a big turn early"));
      p.push({ stage: "inciting", after: s.beats[0]?.key, onScreen: `something happens to ${s.hero ?? "the protagonist"} that they can't ignore`, feel: "concern", why: "a story starts when the balance is upset" });
    } else if (at(inc) > Math.ceil(n / 3)) f.push(finding("mckee:inciting", "warn", inc, `the inciting incident comes at beat ${at(inc) + 1} of ${n}, past the first third`, "bring it earlier, or open on a flash-forward"));
    if (!cri) {
      f.push(finding("mckee:crisis", "error", "story", "there's no crisis: the protagonist never chooses between two goods or two evils", "give the event before the climax a dilemma"));
      const hero = s.hero ? s.agents[s.hero] : undefined;
      p.push({ stage: "crisis", after: map.complications.at(-1) ?? inc, onScreen: `${s.hero ?? "the protagonist"} must choose: ${hero?.want ?? "what they want"}, or ${hero?.need ?? "what they need"}`, feel: "dread", why: "the climax only means something if it was a choice" });
    }
    if (!cli) f.push(finding("mckee:climax", "error", "story", "nothing after the crisis answers it", "show the event the crisis leads to, turning the story's value"));
    else {
      const cliBeat = s.beats.find((b) => b.key === cli)!;
      if (s.focusTurn(cliBeat.turns) === 0) f.push(finding("mckee:climax", "warn", cli, `the climax doesn't turn ${s.focus}, the value the story is about`, `turn ${s.focus} in the climax`));
      const ev = s.events.find((x) => x.key === s.climax)?.ev;
      if (ev && s.hero && ev.who !== s.hero && !(ev.with ?? []).includes(s.hero)) f.push(finding("mckee:climax", "warn", cli, `${s.hero} doesn't act in the climax: ${ev.who} does`, `let ${s.hero} make the decisive move`));
    }
    const comp = map.complications.map((k) => s.beats.find((b) => b.key === k)!.tension);
    for (let i = 1; i < comp.length; i++)
      // easing off: tension falls and no value turns down (danger rising on another value still escalates)
      if (comp[i] < comp[i - 1] - 0.05 && !Object.values(s.beats.find((b) => b.key === map.complications[i])!.turns).some((d) => d < 0)) f.push(finding("mckee:complications", "warn", map.complications[i], "the complications ease off instead of escalating", "make each complication cost more than the last"));
    return { map, findings: f, proposals: p };
  },
});
