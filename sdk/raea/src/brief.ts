// raea/brief: what an agent writes for raea as data (docs/handoff/authoring.md, docs/projects.feature PJ0). A spec is a
// module whose default export is one of these values; it renders nothing and imports nothing that renders.
//
//   // brief.ts
//   import { brief } from "raea/brief";
//   export default brief({ title: "Spring drop", believe: "...", mode: ["pitch"] });
//
// The types are generated from the Rust schemas (core/brief, raea-core-brief) into brief.gen.ts, so what tsc checks is
// what raea reads: `raea context` evaluates the module and Rust validates its default export again.
import type { Brief, Controller, Field, Group, Plan, Script, Taste, Vocabulary } from "./brief.gen";

export type * from "./brief.gen";
export { fingerprint } from "./fingerprint";
export type { Applies, Axis, Fingerprint, Modality } from "./fingerprint";

/** A field as written: its id is filled in from its label when left out. */
export type FieldInput = Field extends infer F ? (F extends Field ? Omit<F, "id"> & { id?: string } : never) : never;
/** A controller as written: ids are filled in from titles and labels when left out. */
export type ControllerInput = Omit<Controller, "id" | "groups"> & {
  id?: string;
  groups: (Omit<Group, "id" | "fields"> & { id?: string; fields: FieldInput[] })[];
};

/** A project's brief (brief.ts): what to make, for whom, the one thing to believe, its deliverables and staged tasks. */
export const brief = (b: Brief): Brief => b;
/** A storyboard or a screenplay (script.ts): scenes, beats, lines, camera and cut. */
export const script = (s: Script): Script => s;
/** Taste (taste.ts): standing rules, some with a check `raea check --taste` holds every export to. */
export const taste = (t: Taste): Taste => t;
/** An object's plan (plan.ts): its parts by name, levels, counts, materials. */
export const plan = (p: Plan): Plan => p;
/** A videoform's vocabulary (vocabulary.ts, docs/videoform.feature VF5): what its context says to look for, written by
 *  the person's AI. `raea videoform report NAME` says what each term did on the references, to revise it from. */
export const vocabulary = (v: Vocabulary): Vocabulary => v;
/** A workspace's controller (controllers/<id>.ts): settings in groups of typed fields that every agent works to.
 *  A standing decision ("every ad is 9:16"), not one piece of work: that's a feed item, and a note on it. The raea app
 *  shows a new controller in the project's feed, and an agent brings one to the person with `controller_post`. */
export const controller = (c: ControllerInput): ControllerInput => c;
