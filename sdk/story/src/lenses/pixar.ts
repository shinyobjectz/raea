// The story spine (Kenn Adams' improv form, which Pixar's story artists use and Emma Coats' rules of storytelling pass
// on): Once upon a time... Every day... Until one day... Because of that... Because of that... Until finally... And ever
// since. Its test is a causal chain with no gaps. Pixar's Braintrust (Catmull, Creativity, Inc.) is a review practice,
// not a structure: candid notes on the problem, never the fix; raea's readers and fingerprint notes play that part.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";

export const pixar = lens({
  name: "pixar",
  about: "The story spine: once upon a time, every day, until one day, because of that (twice or more), until finally, and ever since.",
  stages: ["once", "every-day", "one-day", "because", "finally", "ever-since"],
  read: (_w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const idx = (k: string | undefined) => s.beats.find((b) => b.key === k)?.index ?? -1;
    const one = s.label("pixar", "one-day") ?? s.firstShowing(s.inciting), fin = s.label("pixar", "finally") ?? s.lastShowing(s.climax);
    const routine = s.beats.filter((b) => b.beat.shows.some((e) => s.events.find((x) => x.key === e)?.ev.routine)).map((b) => b.key);
    const because = s.beats.filter((b) => b.index > idx(one) && (fin === undefined || b.index < idx(fin)) && b.linked).map((b) => b.key);
    const ever = fin ? s.beats.filter((b) => b.index > idx(fin)).map((b) => b.key) : [];
    const map = { once: s.beats[0] ? [s.beats[0].key] : [], "every-day": routine, "one-day": one ? [one] : [], because, finally: fin ? [fin] : [], "ever-since": ever };
    if (!routine.length) {
      f.push(finding("pixar:every-day", "warn", "story", "no 'every day': the audience never sees how things are before they change", "show a routine event (routine: true) before the change"));
      p.push({ stage: "every-day", after: undefined, onScreen: `${s.hero ?? "the protagonist"}'s ordinary day: ${s.hero ? s.agents[s.hero]?.want ?? "" : ""}`.trim(), feel: "recognition", why: "a change only reads against the normal it breaks" });
    }
    if (!one) f.push(finding("pixar:one-day", "error", "story", "no 'until one day': nothing breaks the routine", "add the event that changes everything"));
    if (because.length < 2) {
      f.push(finding("pixar:because", "warn", "story", `only ${because.length} 'because of that' between the change and the end`, "add a consequence that leads to the next"));
      p.push({ stage: "because", after: one, onScreen: "a consequence of the change that forces the next move", feel: "concern", why: "the spine needs a chain of consequences, not one jump" });
    }
    for (const b of s.beats) if (b.index > idx(one) && !b.linked) f.push(finding("pixar:because", "error", b.key, `${b.key} breaks the spine: it isn't a consequence of what came before`, "make it follow from an earlier beat"));
    if (!fin) f.push(finding("pixar:finally", "error", "story", "no 'until finally': the chain never resolves", "show the event the chain leads to"));
    if (!ever.length) f.push(finding("pixar:ever-since", "note", "story", "no 'and ever since': the new normal isn't shown", "end on how things are now"));
    return { map, findings: f, proposals: p };
  },
});
