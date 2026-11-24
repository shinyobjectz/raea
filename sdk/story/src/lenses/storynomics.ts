// Storynomics (McKee and Gerace, 2018): stories told for a brand. The customer is the protagonist; the brand is the
// mentor or the gift that helps them act, never the hero of its own ad. The story turns a core value of the customer's
// life from negative to positive, and the customer, not the brand, makes the decisive move. In our words.
import { finding, lens, type Proposal } from "../lens.ts";
import type { Finding } from "../checks.ts";

export const storynomics = lens({
  name: "storynomics",
  about: "The customer is the hero, the brand the mentor or the gift; a core value of the customer's life turns from negative to positive.",
  stages: ["at-stake", "the-gift", "the-act", "restored"],
  read: (w, s) => {
    const f: Finding[] = [];
    const p: Proposal[] = [];
    const customer = Object.keys(s.agents).find((a) => s.agents[a].role === "customer") ?? s.hero;
    const gift = s.brand ? s.beats.find((b) => b.beat.shows.some((e) => { const ev = s.events.find((x) => x.key === e)?.ev; return ev?.who === s.brand || (ev?.with ?? []).includes(s.brand!); }))?.key : undefined;
    const act = s.lastShowing(s.climax);
    const map = { "at-stake": [s.firstShowing(s.inciting)].filter(Boolean) as string[], "the-gift": gift ? [gift] : [], "the-act": act ? [act] : [], restored: s.beats.length ? [s.beats[s.beats.length - 1].key] : [] };
    if (!customer) f.push(finding("storynomics:customer", "error", "story", "there's no customer: the story is about the brand", "make a customer the protagonist (role: \"customer\")"));
    if (!s.brand) {
      f.push(finding("storynomics:brand", "warn", "story", "the brand never appears as mentor or gift", "give the brand an agent (role: \"brand\") that helps the customer act"));
      p.push({ stage: "the-gift", after: map["at-stake"][0], onScreen: `the brand puts something in ${customer ?? "the customer"}'s hands`, feel: "hope", why: "the brand's role is what it gives, not what it is" });
    }
    const climax = s.events.find((x) => x.key === s.climax)?.ev;
    if (climax && s.brand && climax.who === s.brand) f.push(finding("storynomics:hero", "error", act ?? "story", `the brand (${s.brand}) makes the decisive move, stealing ${customer ?? "the customer"}'s climax`, `let ${customer ?? "the customer"} act, with the brand alongside (with: ["${s.brand}"])`));
    const curves = w.resource<Record<string, { levels: number[] }>>("Curves") ?? {};
    const lv = s.focus ? curves[s.focus]?.levels.slice(1) ?? [] : [];
    if (lv.length && !(lv[lv.length - 1] > Math.min(...lv))) f.push(finding("storynomics:value", "error", "story", `${s.focus} doesn't end better than its lowest point`, `end with ${s.focus} turned toward its positive pole`));
    return { map, findings: f, proposals: p };
  },
});
