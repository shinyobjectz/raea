// raea/brief: what an agent writes for raea as data (docs/handoff/authoring.md, docs/projects.feature PJ0). A spec is a
// module whose default export is one of these values; it renders nothing and imports nothing that renders.
//
//   // brief.ts
//   import { brief } from "raea/brief";
//   export default brief({ title: "Spring drop", believe: "...", mode: ["pitch"] });
//
// The types are generated from the Rust schemas (core/brief, raea-core-brief) into brief.gen.ts, so what tsc checks is
// what raea reads: `raea context` evaluates the module and Rust validates its default export again.
import type { Brief, Briefs, Make, Parts, Script, Taste, Vocabulary } from "./brief.gen";

export type * from "./brief.gen";
export { fingerprint } from "./fingerprint";
export type { Applies, Axis, Fingerprint, Modality } from "./fingerprint";

/** A project's brief (brief.ts): what to make, for whom, the one thing to believe and its deliverables. Its tasks are
 *  the project's plan.org (docs/plan.feature PL8a). */
export const brief = (b: Brief): Brief => b;
/** A storyboard or a screenplay (script.ts): scenes, beats, lines, camera and cut. */
export const script = (s: Script): Script => s;
/** Taste (taste.ts): standing rules, the person's, a workspace's or a project's. A rule raea can hold the work to
 *  carries a check: one of six measured forms (`{ form: "text-above", value: 75 }`), one of raea's checks by name
 *  (`"loudness"`), or the project's own by its path from this file (`"../checks/clear.ts"`), with its `values`.
 *  `raea check` runs every one that applies. */
export const taste = (t: Taste): Taste => t;
/** An object's parts (NAME.parts.ts), written before any geometry: each by name, its level, count and material. */
export const parts = (p: Parts): Parts => p;
/** A videoform's vocabulary (vocabulary.ts, docs/videoform.feature VF5): what its context says to look for, written by
 *  the person's AI. `raea videoform report NAME` says what each term did on the references, to revise it from. */
export const vocabulary = (v: Vocabulary): Vocabulary => v;
/** What a videoform's documents say that measuring can't (briefs.ts, docs/videoform.feature VF8f): a constraint, a
 *  negation, an order, a choice, written by the person's AI, each checked against its group's videos. */
export const briefs = (b: Briefs): Briefs => b;

/** A job made to a frozen plan and held to the review loop (docs/make.feature): `make: scene({...})` on brief.ts. Every
 *  object has its real size and whether it came from the reference or was invented; `raea make status` decides when
 *  it's done, and the agent's Stop hook holds it there. */
export const scene = (m: Omit<Make, "kind">): Make => ({ kind: "scene", ...m });
/** A kit: props in one style at real size (`style` is the paragraph every reference carries: era, palette, materials,
 *  wear and one fictional maker). Done is a GLB, a LOD1 and a thumbnail per object, a lineup and kit.json. */
export const kit = (m: Omit<Make, "kind">): Make => ({ kind: "kit", ...m });
/** Shots of a built scene, each named for the file it lands as and described in a crew's words. */
export const shot = (m: Omit<Make, "kind">): Make => ({ kind: "shot", ...m });
