// The launch film: the trouble in the viewer's own words, the name, the product doing its job, a named proof, how to
// start. Its priors are measured (docs/story.md, "What it found"): 156 same-account pairs of launch films on X, the
// better of each pair against the worse. Only length held up there, and length goes with the size of the news, so the
// priors are norms of the films that won, said as notes, never as causes or errors.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";
import { beatsWith } from "./common.ts";

/** What the launch pairs measured (11-22, ~/.vd/launch, tools/compare.py and the blind read). */
export const LAUNCH_PRIORS = {
  pairs: 156,
  seconds: { better: { q1: 30.5, median: 50.5, q3: 80.3 }, worse: { q1: 24, median: 33.1, q3: 56 } },
  /** how often a film of this length was the better of its pair */
  betterByLength: [
    { upTo: 20, share: 0.39 },
    { upTo: 40, share: 0.35 },
    { upTo: 60, share: 0.55 },
    { upTo: 90, share: 0.64 },
    { upTo: Infinity, share: 0.73 },
  ],
  voice: { better: 0.4, worse: 0.29, p: 0.043 },
  lengthSignTest: { higher: 104, of: 155, p: 0.0001 },
} as const;

export const launch = lens({
  name: "launch",
  about: "The trouble in the viewer's words, the name, the product doing its job, a named proof, how to start; priors measured on 156 launch pairs.",
  stages: ["trouble", "name", "works", "proof", "start"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const n = s.beats.length;
    const down = (t: Record<string, number>) => Object.values(t).some((d) => d < 0);
    const branded = beatsWith(s, s.brand);
    const trouble = s.beats.find((b) => down(b.turns) && !branded.includes(b));
    const name = branded[0];
    const works = branded.find((b) => s.focusTurn(b.turns) > 0);
    const proof = s.beats.find((b) => b.beat.shows.some((e) => (s.events.find((x) => x.key === e)?.ev.proof ?? []).length > 0));
    const start = n > 1 && s.beats[n - 1] !== trouble ? s.beats[n - 1] : undefined;
    const map = { trouble: trouble ? [trouble.key] : [], name: name ? [name.key] : [], works: works ? [works.key] : [], proof: proof ? [proof.key] : [], start: start ? [start.key] : [] };

    if (!trouble) {
      f.push(finding("launch:trouble", "warn", "story", "no trouble: nothing goes wrong for the viewer before the product", "open on the problem in the viewer's own words"));
      p.push({ stage: "trouble", onScreen: `${s.hero ?? "the viewer"} stuck on the problem today`, feel: "recognition", why: "a launch is read as an answer only after the question" });
    } else if (trouble.index > Math.floor(n / 3)) f.push(finding("launch:trouble", "note", trouble.key, `the trouble comes at beat ${trouble.index + 1} of ${n}`, "say it in the first third"));
    if (!s.brand) f.push(finding("launch:name", "note", "story", "no agent has the brand role, so the product is never named", "give the product's agent role: brand"));
    else if (!works) f.push(finding("launch:works", "warn", name?.key ?? "story", "the product never does its job on screen", "show it working on the viewer's problem"));
    if (!proof) {
      f.push(finding("launch:proof", "note", works?.key ?? "story", "no named proof: no claim the story makes true (a number, a benchmark, a customer)", "back the claim with a fact the story shows"));
      p.push({ stage: "proof", after: works?.key, onScreen: "the number that proves it", feel: "trust", why: "the better launches named their proof" });
    }

    // the measured priors, as notes: length (when every beat says how long it is) and a voice
    const secs = s.beats.every((b) => typeof b.beat.seconds === "number") ? s.beats.reduce((a, b) => a + (b.beat.seconds as number), 0) : null;
    if (secs !== null) {
      const L = LAUNCH_PRIORS.seconds.better, at = LAUNCH_PRIORS.betterByLength.find((x) => secs <= x.upTo)!;
      if (secs < L.q1) f.push(finding("launch:length", "note", "story", `${secs.toFixed(0)} s: shorter than three in four of the launches that did better (median ${L.median} s); films this long were the better of their pair ${Math.round(at.share * 100)}% of the time`, "length goes with the size of the news; cut nothing to fit and pad nothing to reach it"));
    }
    if (!s.beats.some((b) => (b.beat.says ?? []).length > 0)) f.push(finding("launch:voice", "note", "story", `nobody speaks: ${LAUNCH_PRIORS.voice.better * 100}% of the better launches had a voice, ${LAUNCH_PRIORS.voice.worse * 100}% of the worse (p = ${LAUNCH_PRIORS.voice.p}, not significant across 23 measures)`, "a founder or a voice saying why can help; it's not required"));
    return { map, findings: f, proposals: p };
  },
});
