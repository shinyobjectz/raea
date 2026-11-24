// Bernays's Propaganda (1928, public domain), in our words and on our terms. He described persuasion as arranging
// circumstances that make a claim evident (an event the public sees), carried by voices the audience already trusts,
// and tied to what people already want. raea keeps the craft and refuses the rest: a claim is only ever arranged proof,
// never invented. So this lens checks that every claim's proof is an event shown on screen, not a line said; that
// someone besides the brand carries or witnesses it; and that it meets the customer's own want.
import { beatsOf } from "../load.ts";
import { finding, lens } from "../lens.ts";
import type { Audience } from "../systems/know.ts";
import type { Finding } from "../checks.ts";
import { at } from "./common.ts";

export const bernays = lens({
  name: "bernays",
  about: "Proof arranged as an event the audience sees, carried by a voice they trust, tied to what they already want. Never invented.",
  stages: ["desire", "circumstance", "witness", "claim"],
  read: (w, s) => {
    // the first beat by which the audience knows every fact given
    const knownBy = (facts: string[]) => beatsOf(w).find(({ e }) => facts.every((x) => w.must<Audience>(e, "Audience").knows.includes(x)))?.key;
    const f: Finding[] = [];
    const claims = s.events.filter((x) => x.ev.claim);
    const customer = Object.keys(s.agents).find((a) => s.agents[a].role === "customer") ?? s.hero;
    const map: Record<string, string[]> = { desire: [], circumstance: [], witness: [], claim: [] };
    for (const c of claims) {
      // where the claim lands: its last showing (a hook can show it early, as a promise)
      const claimBeat = s.lastShowing(c.key);
      if (claimBeat) map.claim.push(claimBeat);
      const proof = c.ev.proof ?? [];
      const setters = s.events.filter((x) => x.order <= c.order && (x.ev.sets ?? []).some((p) => proof.includes(p)));
      if (proof.length && !setters.length) {
        f.push(finding("bernays:invented", "error", c.key, `the claim "${c.ev.claim}" rests on proof no event makes true: that's invented, not arranged`, "show the event that makes the proof true, or cut the claim"));
        continue;
      }
      const seen = proof.length ? knownBy(proof) : undefined;
      if (seen) map.circumstance.push(seen);
      if (proof.length && (!seen || at(s, seen) > at(s, claimBeat))) f.push(finding("bernays:circumstance", "warn", claimBeat ?? c.key, `the claim "${c.ev.claim}" is said before its proof is ever seen`, "show the proof's event on screen at or before the claim"));
      const voices = new Set(setters.flatMap((x) => [x.ev.who, ...(x.ev.with ?? [])]));
      if (s.brand && voices.size && [...voices].every((v) => v === s.brand)) f.push(finding("bernays:witness", "warn", c.key, `only ${s.brand} carries the proof of "${c.ev.claim}"`, "let the customer, or someone they trust, make or witness it"));
      else for (const x of setters) if (x.ev.who !== s.brand) { const b = s.lastShowing(x.key); if (b) map.witness.push(b); }
      const meetsWant = customer && s.events.some((x) => x.order >= c.order - 1 && x.ev.ends?.[customer] && (x.key === c.key || setters.some((y) => y.key === x.key) || x.order === c.order));
      if (customer && s.agents[customer]?.want && !meetsWant) f.push(finding("bernays:desire", "note", c.key, `the claim "${c.ev.claim}" doesn't meet ${customer}'s want (${s.agents[customer].want})`, "tie the claim to the event that settles what the customer wants"));
    }
    if (customer) { const b = s.beats[0]?.key; if (b) map.desire.push(b); }
    return { map, findings: f, proposals: [] };
  },
});
