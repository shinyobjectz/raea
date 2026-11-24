// Fuzz the contract "anything fromData accepts, every reader can read": take valid stories, put a wrong-typed value in
// one field at a time, and run every reader on what fromData lets through. A crash is a hole in fromData or a reader.
//   bun bench/fuzz.ts [N]
import { asker, checkStory, compileScript, fromData, LENSES, readThrough, rng, toData, toLines, windowAt, type AnyStory } from "../src/index.ts";
import { launch } from "../test/fixture.ts";

type J = Record<string, any>;
const N = Number(process.argv[2] ?? 3000);
const seeds: J[] = [toData(launch as AnyStory) as J];
const WEIRD: unknown[] = [{}, [], "", 0, 5, null, true, "x", [{}], [null], { a: 1 }, -1, 1.5, ["nope"]];
const r = rng(7);
const paths = (o: unknown, at: (string | number)[] = []): (string | number)[][] =>
  o && typeof o === "object" ? [at, ...Object.entries(o).flatMap(([k, v]) => paths(v, [...at, Array.isArray(o) ? Number(k) : k]))] : [at];
const crashes = new Map<string, { path: string; value: string; reader: string; n: number }>();
let accepted = 0;
for (let i = 0; i < N; i++) {
  const d = structuredClone(seeds[0]);
  // half the time an existing field, half a field the story could have but doesn't (every optional field gets a turn)
  const EV = ["who", "with", "does", "because", "needs", "sets", "unsets", "learns", "turns", "ends", "dilemma", "claim", "proof", "routine", "surprise"];
  const BEAT = ["id", "shows", "reveals", "hides", "asks", "feel", "setup", "pays", "stage", "surprise", "meant", "says", "onScreen", "seconds", "scene"];
  const AG = ["want", "need", "lie", "mask", "role", "believes", "arc"];
  const TOP = ["idea", "facts", "lenses", "title", "premise"];
  const pick = <T,>(xs: T[]) => xs[Math.floor(r() * xs.length)];
  const ps = paths(d).filter((p) => p.length > 0);
  const t0 = Object.keys(d.tellings)[0];
  const p = r() < 0.5 ? ps[Math.floor(r() * ps.length)]
    : pick([
        () => ["world", pick(Object.keys(d.world)), pick(EV)],
        () => ["tellings", t0, Math.floor(r() * d.tellings[t0].length), pick(BEAT)],
        () => ["tellings", t0, Math.floor(r() * d.tellings[t0].length), "says", 0, pick(["who", "says", "means", "how"])],
        () => ["agents", pick(Object.keys(d.agents)), pick(AG)],
        () => [pick(TOP)],
        () => ["idea", pick(["value", "side", "counter"])],
      ])();
  if (p[p.length - 2] === 0 && p[p.length - 3] === "says") { let o: any = d; for (const k of p.slice(0, -3)) o = o[k]; o.says ??= [{ who: "narrator", says: "hi" }]; }
  const v = WEIRD[Math.floor(r() * WEIRD.length)];
  let o: any = d;
  for (const k of p.slice(0, -1)) o = o[k];
  o[p[p.length - 1]] = structuredClone(v);
  const { story, errors } = fromData(d);
  if (!story || errors.length) continue;
  accepted++;
  const t = Object.keys((story as J).tellings)[0];
  const readers: [string, () => unknown][] = [
    ["checkStory", () => checkStory(story, t)],
    ["lenses", () => readThrough(story, Object.values(LENSES), t)],
    ["toLines", () => toLines(story)],
    ["windowAt", () => windowAt(story, t, 1)],
    ["asker", () => asker(story, t)(0)],
    ["compileScript", () => compileScript(story, t)],
  ];
  for (const [name, fn] of readers) {
    try { fn(); } catch (e) {
      if (e instanceof RangeError) continue; // a clear refusal (asking about a beat that isn't there), not a crash
      const where = p.map((k) => (typeof k === "number" ? "[]" : k)).join(".").replace(/world\.\w+/, "world.E").replace(/agents\.\w+/, "agents.A").replace(/tellings\.\w+/, "tellings.T");
      const key = `${where} = ${JSON.stringify(v)} | ${name} | ${String((e as Error).message).slice(0, 60)}`;
      const c = crashes.get(key);
      if (c) c.n++; else crashes.set(key, { path: where, value: JSON.stringify(v), reader: name, n: 1 });
    }
  }
}
console.log(`${N} mutations, ${accepted} accepted by fromData, ${crashes.size} distinct crashes`);
for (const k of [...crashes.keys()].sort()) console.log(`  ${k}  (x${crashes.get(k)!.n})`);
