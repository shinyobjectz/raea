// Edge cases for raea/story: broken, extreme and odd stories, each with what the SDK should do, run, and what it did.
// Not a test suite (a case can fail and the run goes on); the cases that hold become tests.
//   bun bench/edges.ts [--json out.json]
import { writeFileSync } from "node:fs";
import {
  asker, checkStory, choose, compileScript, compileStorylets, dramaManager, fromData, fromLines, LENSES, mutants, readThrough,
  samples, stateAt, story, toInk, toLines, value, windowAt, type AnyStory,
} from "../src/index.ts";

type J = Record<string, any>;
type Result = { name: string; expect: string; got: string; ok: boolean | null; ms: number };
const out: Result[] = [];
const S = (j: J) => ({ ...j, kind: "story" }) as unknown as AnyStory;
const brief = (fs: { check: string; level: string; about: string }[]) => fs.filter((f) => f.level !== "note").map((f) => `${f.level}:${f.check}@${f.about}`).join(" ") || "clean";
function run(name: string, expect: string, fn: () => { got: string; ok: boolean | null }) {
  const t = performance.now();
  try { const r = fn(); out.push({ name, expect, ...r, ms: Math.round(performance.now() - t) }); }
  catch (e) { out.push({ name, expect, got: `CRASH ${String((e as Error).message ?? e).slice(0, 140)}`, ok: false, ms: Math.round(performance.now() - t) }); }
}
const base = { values: { v: value("low", "high") }, agents: { a: { want: "x", need: "y" } } };

// ---- shape edges ----
run("empty story", "no crash; says there's nothing", () => {
  const f = checkStory(S({ values: {}, agents: {}, world: {}, tellings: { t: [] } }), "t");
  return { got: brief(f), ok: true };
});
run("telling with zero beats, real world", "no crash", () => {
  const f = checkStory(S({ ...base, world: { e: { who: "a", does: "x", turns: { v: 1 } } }, tellings: { t: [] } }), "t");
  return { got: brief(f), ok: true };
});
run("all narration (every beat shows nothing)", "turns warnings, no crash", () => {
  const f = checkStory(S({ ...base, world: { e: { who: "a", does: "x" } }, tellings: { t: [{ shows: [] }, { shows: [] }] } }), "t");
  return { got: brief(f), ok: f.some((x) => x.check === "turns") };
});
run("fromData: beat shows an event that doesn't exist", "refused with where", () => {
  const r = fromData({ ...base, world: { e: { who: "a", does: "x" } }, tellings: { t: [{ shows: ["ghost"] }] } });
  return { got: r.errors.map((e) => `${e.at}: ${e.says}`).join("; "), ok: r.errors.some((e) => e.at.includes("shows")) };
});
run("bypassing fromData: beat shows a missing event", "a finding or a clear error, not a silent pass", () => {
  const f = checkStory(S({ ...base, world: { e: { who: "a", does: "x", turns: { v: 1 } } }, tellings: { t: [{ shows: ["ghost"] }] } }), "t");
  return { got: brief(f), ok: f.length > 0 };
});
run("cause cycle (a because b, b because a)", "fromData refuses (cause after effect)", () => {
  const r = fromData({ ...base, world: { a1: { who: "a", does: "x", because: "b1" }, b1: { who: "a", does: "y", because: "a1" } }, tellings: { t: [{ shows: ["a1", "b1"] }] } });
  return { got: r.errors.map((e) => e.says).join("; ") || "accepted", ok: r.errors.length > 0 };
});
run("self-cause (a because a)", "refused", () => {
  const r = fromData({ ...base, world: { a1: { who: "a", does: "x", because: "a1" } }, tellings: { t: [{ shows: ["a1"] }] } });
  return { got: r.errors.map((e) => e.says).join("; ") || "accepted", ok: r.errors.length > 0 };
});
run("bypassing fromData: cycle straight into checkStory", "no hang, no crash", () => {
  const f = checkStory(S({ ...base, world: { a1: { who: "a", does: "x", because: "b1", turns: { v: 1 } }, b1: { who: "a", does: "y", because: "a1", turns: { v: -1 } } }, tellings: { t: [{ shows: ["a1"] }, { shows: ["b1"] }] } }), "t");
  return { got: brief(f), ok: f.some((x) => x.check === "cause") };
});
run("turn out of range (+9)", "fromData refuses", () => {
  const r = fromData({ ...base, world: { e: { who: "a", does: "x", turns: { v: 9 } } }, tellings: { t: [{ shows: ["e"] }] } });
  return { got: r.errors.map((e) => e.says).join("; ") || "accepted", ok: r.errors.length > 0 };
});
run("duplicate beat ids", "flagged (raea check finds Sequences by id)", () => {
  const r = fromData({ ...base, world: { e: { who: "a", does: "x", turns: { v: 1 } }, f: { who: "a", does: "y", because: "e", turns: { v: 1 } } }, tellings: { t: [{ id: "x", shows: ["e"] }, { id: "x", shows: ["f"] }] } });
  const f = r.story ? checkStory(r.story, "t") : [];
  return { got: (r.errors.map((e) => e.says).join("; ") || "accepted") + " | " + brief(f), ok: r.errors.length > 0 || f.some((x) => x.about === "x") };
});
run("an agent named 'narrator'", "no confusion with the narrator", () => {
  const r = fromData({ values: base.values, agents: { narrator: {} }, world: { e: { who: "narrator", does: "x", turns: { v: 1 } } }, tellings: { t: [{ shows: ["e"], says: [{ who: "narrator", says: "hi" }] }] } });
  return { got: (r.errors.map((e) => e.says).join("; ") || "accepted") + " | " + (r.story ? brief(checkStory(r.story, "t")) : ""), ok: null };
});
run("unicode keys and Japanese text", "works; lines round-trip", () => {
  const j = { values: { 勇気: { neg: "怖い", pos: "勇敢" } }, agents: { 花子: {} }, world: { 登る: { who: "花子", does: "山に登る 🏔️ | 一人で", turns: { 勇気: 2 } } }, tellings: { t: [{ shows: ["登る"], onScreen: "雪の中\nもう一歩" }] } };
  const r = fromData(j), back = fromLines(toLines(r.story!));
  return { got: `${brief(checkStory(r.story!, "t"))} | roundtrip errors ${back.errors.length}, does="${(back.story as any)?.world?.["登る"]?.does}"`, ok: back.errors.length === 0 && (back.story as any).world["登る"].does === j.world["登る"].does };
});
run("negative seconds on a beat", "flagged", () => {
  const r = fromData({ ...base, world: { e: { who: "a", does: "x", turns: { v: 1 } } }, tellings: { t: [{ shows: ["e"], seconds: -3 }] } });
  return { got: r.errors.map((e) => e.says).join("; ") || "accepted", ok: r.errors.length > 0 };
});

// ---- story edges ----
const mystery = (learnsEarly: boolean) => S({
  values: { truth: value("hidden", "known") },
  facts: { poison: "the tea was poisoned" },
  agents: { det: { want: "the killer", need: "to trust a stranger" }, maid: {} },
  world: {
    pour: { who: "maid", does: "pours the tea", sets: ["poison"], turns: { truth: -1 } },
    ...(learnsEarly ? { smell: { who: "det", does: "smells bitter almonds", because: "pour", learns: ["poison"], turns: { truth: 1 } } } : {}),
    accuse: { who: "det", does: "accuses the maid of poisoning", because: learnsEarly ? "smell" : "pour", needs: ["poison"], turns: { truth: 2 } },
  },
  tellings: { t: [{ shows: ["pour"], hides: ["poison"], asks: ["poison"] }, ...(learnsEarly ? [{ shows: ["smell"] }] : []), { shows: ["accuse"], reveals: ["poison"] }] },
});
run("mystery: detective acts on what he can't know", "belief error", () => { const f = checkStory(mystery(false), "t"); return { got: brief(f), ok: f.some((x) => x.check === "belief") }; });
run("mystery: fair-play clue added", "belief error gone", () => { const f = checkStory(mystery(true), "t"); return { got: brief(f), ok: !f.some((x) => x.check === "belief") }; });

const twoCuts = S({
  ...base,
  world: {
    work: { who: "a", does: "works late again", routine: true, turns: { v: -1 } },
    quit: { who: "a", does: "quits", because: "work", turns: { v: 2 }, ends: { a: "want" } },
    open: { who: "a", does: "opens a bakery", because: "quit", turns: { v: 2 }, ends: { a: "need" } },
  },
  tellings: { long: [{ id: "w", shows: ["work"] }, { id: "q", shows: ["quit"] }, { id: "o", shows: ["open"] }], short: [{ id: "w", shows: ["work"] }] },
});
run("two cuts of one world: a cut that stops early", "the short cut's unsettled arc shows only in it", () => {
  const L = checkStory(twoCuts, "long"), Sh = checkStory(twoCuts, "short");
  return { got: `long: ${brief(L)} || short: ${brief(Sh)}`, ok: brief(L) !== brief(Sh) };
});
run("flashback: show the ending first as a hook", "passes when the ending's fact is hidden and paid", () => {
  const s = S({ ...base, facts: { done: "the bakery is open" }, world: { ...twoCuts.world, open: { ...twoCuts.world.open, sets: ["done"] } },
    tellings: { t: [{ id: "hook", shows: ["open"], hides: ["done"], asks: ["done"], setup: "how" }, { id: "w", shows: ["work"] }, { id: "q", shows: ["quit"] }, { id: "o", shows: ["open"], reveals: ["done"], pays: "how" }] } });
  const f = checkStory(s, "t");
  return { got: brief(f), ok: !f.some((x) => x.level === "error") };
});
run("tragedy: negative arc, need never met", "no arc error for a meant negative arc", () => {
  const s = S({ values: { hope: value("despair", "hope") }, agents: { k: { want: "the crown", need: "to let go", arc: "negative" } },
    world: { take: { who: "k", does: "takes the crown", turns: { hope: 1 }, ends: { k: "want" } }, lose: { who: "k", does: "loses everyone", because: "take", turns: { hope: -3 } } },
    tellings: { t: [{ shows: ["take"] }, { shows: ["lose"] }] } });
  const f = checkStory(s, "t");
  return { got: brief(f), ok: !f.some((x) => x.check === "arc" && x.level !== "note") };
});
run("kishotenketsu: no conflict, a turn of view", "kishotenketsu passes; mckee may complain", () => {
  const s = S({ values: { view: value("ordinary", "wonder") }, agents: { girl: {}, old: {} },
    world: { ki: { who: "girl", does: "waters a plant on a balcony", routine: true }, sho: { who: "girl", does: "waters it every day as it grows", because: "ki", turns: { view: 1 } },
      ten: { who: "old", does: "across the street, an old man has drawn the plant every day for a year", turns: { view: 2 } }, ketsu: { who: "girl", does: "waves; he holds up the drawings", because: "ten", turns: { view: 1 } } },
    tellings: { t: [{ shows: ["ki"] }, { shows: ["sho"] }, { shows: ["ten"], surprise: "meant" }, { shows: ["ketsu"] }] } });
  const r = readThrough(s, [LENSES.kishotenketsu, LENSES.mckee], "t");
  return { got: r.map((x) => `${x.lens}:${x.passes ? "pass" : "FAIL"} ${brief(x.findings)}`).join(" || "), ok: r[0].passes };
});
run("claim before its proof", "claim error", () => {
  const s = S({ ...base, facts: { fast: "it ships in a day" }, agents: { a: {}, brand: { role: "brand" } },
    world: { claim: { who: "brand", does: "says it ships in a day", claim: "ships in a day", proof: ["fast"], turns: { v: 1 } }, ship: { who: "brand", does: "ships the order in a day", because: "claim", sets: ["fast"], turns: { v: 1 } } },
    tellings: { t: [{ shows: ["claim"] }, { shows: ["ship"] }] } });
  const f = checkStory(s, "t");
  return { got: brief(f), ok: f.some((x) => x.check === "claim") };
});
run("unreliable narrator: the audience is told something false, then it's undone", "a way to say it (unsets)", () => {
  const s = S({ values: { trust: value("doubt", "trust") }, facts: { alibi: "he was home all night" }, agents: { him: {} },
    world: { claims: { who: "him", does: "says he was home", sets: ["alibi"], turns: { trust: 1 } }, cam: { who: "him", does: "a camera shows him leaving at midnight", because: "claims", unsets: ["alibi"], turns: { trust: -3 } } },
    tellings: { t: [{ shows: ["claims"] }, { shows: ["cam"] }] } });
  const st = stateAt(s, "t", 2);
  return { got: `${brief(checkStory(s, "t"))} | audience knows at the end: [${st.knows}]`, ok: !st.knows.includes("alibi") };
});
run("surprise not meant vs meant", "warn, then nothing", () => {
  const mk = (meant: boolean) => S({ ...base, world: { a1: { who: "a", does: "x", turns: { v: 1 } }, hidden: { who: "a", does: "unseen" }, z: { who: "a", does: "a meteor", because: "hidden", turns: { v: -3 } } },
    tellings: { t: [{ shows: ["a1"] }, { shows: ["z"], ...(meant ? { surprise: "meant" } : {}) }] } });
  return { got: `${brief(checkStory(mk(false), "t"))} || ${brief(checkStory(mk(true), "t"))}`, ok: null };
});

// ---- generation edges ----
run("choice points: 20 seeds", "several distinct stories, the failing ones refused", () => {
  const s = story({
    values: { v: value("low", "high") },
    agents: { a: { want: choose(["the job", "the girl", "the boat"]), need: "y" } },
    world: { e: { who: "a", does: choose(["runs", "hides", "lies"]), turns: { v: choose([-2, 1, 2]) } }, f: { who: "a", does: "comes back", because: "e", turns: { v: 2 } } },
    tellings: { t: [{ shows: ["e"] }, choose([{ shows: ["f"] }, null])] },
  } as any);
  const xs = samples(s, 20, { telling: "t" });
  const distinct = new Set(xs.map((x) => JSON.stringify(x.story))).size;
  return { got: `${xs.length} sampled, ${distinct} distinct, ${xs.filter((x) => x.passes).length} pass`, ok: distinct > 1 };
});
run("mutants of the mystery", "every mutant is a story the checks can read", () => {
  const ms = mutants(mystery(true), "t");
  let found = 0, crash = 0;
  for (const m of ms) { try { if (checkStory(m.story, m.telling).length) found++; } catch { crash++; } }
  return { got: `${ms.length} mutants (${[...new Set(ms.map((m) => m.label.kind))].join(",")}), ${found} raise a finding, ${crash} crash`, ok: crash === 0 };
});
run("compile a branching story to a game", "storylets, a drama-manager path, ink", () => {
  const sl = compileStorylets(twoCuts);
  const path = dramaManager(sl, [0.2, 0.5, 0.9], 1);
  const ink = toInk(twoCuts, "long");
  const sc = compileScript(twoCuts, "long");
  return { got: `${sl.length} storylets, path ${path.join(">")}, ink ${ink.split("\n").length} lines, script ${sc.beats.length} beats`, ok: sl.length === 3 && ink.length > 0 };
});

// ---- scale ----
function big(n: number, beatsPer: number): AnyStory {
  const world: J = {}, tell: J[] = [];
  for (let i = 0; i < n; i++) world[`e${i}`] = { who: i % 2 ? "a" : "b", does: `step ${i}`, ...(i ? { because: `e${i - 1}` } : {}), turns: { v: i % 3 === 0 ? -1 : 1 } };
  for (let i = 0; i < n; i++) for (let k = 0; k < beatsPer; k++) tell.push({ id: `s${i}_${k}`, shows: [`e${i}`], scene: `sc${Math.floor(i / 10)}` });
  return S({ values: base.values, agents: { a: {}, b: {} }, world, tellings: { t: tell } });
}
for (const [n, per] of [[200, 1], [500, 4], [2000, 1]] as const) {
  run(`scale: ${n} events, ${n * per} beats`, "checks, lenses, a window and lines in seconds", () => {
    const s = big(n, per);
    const t0 = performance.now(); const f = checkStory(s, "t"); const t1 = performance.now();
    readThrough(s, [LENSES.mckee], "t"); const t2 = performance.now();
    const w = windowAt(s, "t", Math.floor((n * per) / 2)); const t3 = performance.now();
    const l = toLines(s); const t4 = performance.now();
    return { got: `checks ${(t1 - t0).toFixed(0)} ms (${f.length} findings), mckee ${(t2 - t1).toFixed(0)} ms, window ${(t3 - t2).toFixed(0)} ms (${w.length} chars), lines ${(t4 - t3).toFixed(0)} ms`, ok: t4 - t0 < 30000 };
  });
}
run("questions on a 2,000-beat telling", "a beat's questions without reading the whole telling each time", () => {
  const s = big(2000, 1);
  const t0 = performance.now(); const ask = asker(s, "t"); const t1 = performance.now(); const q = ask(1000, "lines"); const t2 = performance.now();
  return { got: `setup ${(t1 - t0).toFixed(0)} ms, one beat ${(t2 - t1).toFixed(0)} ms, ${Object.keys(q.questions).length} questions, state ${q.state.length} chars`, ok: t2 - t1 < 2000 };
});

const pad = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s.padEnd(n));
for (const r of out) console.log(`${r.ok === true ? "ok  " : r.ok === false ? "FAIL" : " ?  "} ${pad(r.name, 52)} ${String(r.ms).padStart(6)} ms  ${r.got.slice(0, 260)}`);
console.log(`\n${out.filter((r) => r.ok === true).length} ok, ${out.filter((r) => r.ok === false).length} fail, ${out.filter((r) => r.ok === null).length} to read`);
const j = process.argv.indexOf("--json");
if (j > 0) writeFileSync(process.argv[j + 1], JSON.stringify(out, null, 1));
