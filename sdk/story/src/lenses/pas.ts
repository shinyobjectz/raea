// Problem, agitate, solve: the ad and launch-video form. The problem is the audience's own, stated early; agitating it
// makes it cost more; the solve shows the product doing the job, with proof. Its priors (how early, how long) are what
// the launch lens learns from real outcomes (docs/story.feature ST7a); these are the form's own rules of thumb.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";

export const pas = lens({
  name: "pas",
  about: "Problem (the audience's own, early), agitate (it costs more), solve (the product doing the job, with proof).",
  stages: ["problem", "agitate", "solve"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const neg = (t: Record<string, number>) => Object.values(t).some((d) => d < 0);
    const involves = (b: (typeof s.beats)[number], who: string | undefined) => !!who && b.beat.shows.some((e) => { const ev = s.events.find((x) => x.key === e)?.ev; return ev?.who === who || (ev?.with ?? []).includes(who); });
    const problemB = s.beats.find((b) => neg(b.turns) && !involves(b, s.brand));
    const solveB = problemB ? s.beats.find((b) => b.index > problemB.index && s.focusTurn(b.turns) > 0) : undefined;
    const agitate = problemB ? s.beats.filter((b) => b.index > problemB.index && (!solveB || b.index < solveB.index) && neg(b.turns)) : [];
    const map = { problem: problemB ? [problemB.key] : [], agitate: agitate.map((b) => b.key), solve: solveB ? [solveB.key] : [] };
    const n = s.beats.length;
    if (!problemB) {
      f.push(finding("pas:problem", "error", "story", "no problem: no beat turns anything down before the product", "open on the cost the audience pays today"));
      p.push({ stage: "problem", onScreen: `${s.hero ?? "the audience"} paying the cost today: ${s.hero ? s.agents[s.hero]?.want ?? "" : ""}`.trim(), feel: "recognition", why: "the audience has to see their own problem before they want a fix" });
    } else if (problemB.index > Math.floor(n * 0.4)) f.push(finding("pas:problem", "warn", problemB.key, `the problem arrives at beat ${problemB.index + 1} of ${n}, late for an ad`, "state it in the first beats"));
    if (problemB && !agitate.length) {
      f.push(finding("pas:agitate", "note", "story", "the problem is stated but never gets worse", "show what it costs next"));
      p.push({ stage: "agitate", after: problemB.key, onScreen: "what the problem costs next, if nothing changes", feel: "concern", why: "agitation makes the solve feel earned" });
    }
    if (!solveB) f.push(finding("pas:solve", "error", "story", "no solve: nothing turns the problem around", "show the product doing the job"));
    else {
      if (!involves(solveB, s.brand)) f.push(finding("pas:solve", "warn", solveB.key, "the solve doesn't show the product", "put the product in the beat that turns things around"));
      const proven = solveB.beat.shows.some((e) => (s.events.find((x) => x.key === e)?.ev.proof ?? []).length > 0);
      if (!proven) f.push(finding("pas:proof", "note", solveB.key, "the solve makes no claim backed by a fact", "show proof: a fact the story makes true"));
    }
    return { map, findings: f, proposals: p };
  },
});
