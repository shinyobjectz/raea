// The story model (docs/story.feature ST0a): a story is data in three layers. The world holds facts and the events that
// change them, each event with the event that caused it, in story order. A telling is a list of beats: which events the
// audience sees, in what order, what each one reveals and the feeling it aims for. The render is raea/brief's Script,
// compiled from a telling (compile.ts). Every reference is a typed key, so tsc refuses a story that names an agent, a
// fact, a value, an event or a setup it doesn't declare. Keys are inferred where they're declared and checked
// (NoInfer) where they're used.

/** A quality at stake, with two poles (McKee's value): `value("buried", "free")`. */
export type Value = { neg: string; pos: string };
export const value = (neg: string, pos: string): Value => ({ neg, pos });

/** A choice point: the story holds options, and sampling picks one with a seed (sample.ts). */
export type Choice<T> = { readonly choice: readonly T[]; readonly weights?: readonly number[] };
export const choose = <const T>(options: readonly T[], weights?: readonly number[]): Choice<T> => ({ choice: options, weights });
export const isChoice = (x: unknown): x is Choice<unknown> =>
  typeof x === "object" && x !== null && Array.isArray((x as { choice?: unknown }).choice);
/** A field that may be fixed or a choice point. */
export type Maybe<T> = T | Choice<T>;

/** The masks a character wears in a beat (Campbell and Vogler's archetypes as functions, not types). */
export type Mask = "hero" | "mentor" | "ally" | "herald" | "threshold" | "shapeshifter" | "shadow" | "trickster" | "gift";

/** The feeling a beat aims for in the audience. An open set; these are the ones the lenses read. */
export type Feeling =
  | "curiosity" | "recognition" | "concern" | "dread" | "hope" | "relief" | "delight" | "surprise" | "awe" | "resolve"
  | (string & {});

export type Agent<F extends string> = {
  /** what they consciously want */
  want?: string;
  /** what they need, often against the want */
  need?: string;
  /** the lie they believe at the start */
  lie?: string;
  mask?: Mask;
  /** facts they hold true from the start */
  believes?: NoInfer<F>[];
  arc?: "positive" | "negative" | "flat";
  /** the customer, the brand: Storynomics reads these */
  role?: "customer" | "brand" | (string & {});
};

export type Event<V extends string, F extends string, A extends string, E extends string> = {
  who: NoInfer<A>;
  /** others who act with them (the brand helping the customer act, Storynomics) */
  with?: NoInfer<A>[];
  does: Maybe<string>;
  /** the earlier event or events that caused this one; every event after the first has one */
  because?: NoInfer<E> | NoInfer<E>[];
  /** facts `who` acts on: they must hold them when this happens */
  needs?: NoInfer<F>[];
  /** facts this makes true (everyone in the event learns them) */
  sets?: NoInfer<F>[];
  /** facts this makes false */
  unsets?: NoInfer<F>[];
  /** facts `who` comes to know without making them true */
  learns?: NoInfer<F>[];
  /** how far each value moves, in steps (+2 is a big turn toward the positive pole) */
  turns?: Partial<Record<NoInfer<V>, number>>;
  /** what this settles for an agent: their want met or refused, their need met, their lie dropped */
  ends?: Partial<Record<NoInfer<A>, "want" | "need" | "lie" | "want-refused">>;
  /** a dilemma: the options the agent chooses between (McKee's crisis) */
  dilemma?: string[];
  /** a claim made to the audience; `proof` names the facts behind it (Bernays: arrange proof, never invent it) */
  claim?: string;
  proof?: NoInfer<F>[];
  /** a routine event: how things are every day (Pixar's spine) */
  routine?: boolean;
};

export type Line<A extends string> = {
  who: NoInfer<A> | "narrator";
  says: Maybe<string>;
  /** the subtext: what's meant */
  means?: string;
  /** how it's said (a screenplay's parenthetical) */
  how?: string;
};

export type Beat<F extends string, A extends string, E extends string, S extends string> = {
  /** its name: what a composition's Sequence and raea check call it (default b0, b1 ... by place) */
  id?: string;
  /** the events the audience sees; a beat with none is narration or a title over them */
  shows: NoInfer<E>[];
  /** facts the audience learns here beyond what the shown events set */
  reveals?: NoInfer<F>[];
  /** facts a shown event sets that the audience does NOT learn here (a secret) */
  hides?: NoInfer<F>[];
  /** questions the beat raises: facts the audience now wants to know */
  asks?: NoInfer<F>[];
  feel?: Maybe<Feeling>;
  /** a setup this beat plants, by a name of the story's choosing */
  setup?: S | S[];
  /** a setup this beat pays off */
  pays?: NoInfer<S> | NoInfer<S>[];
  /** this beat's stage in a lens, by lens name, when the lens shouldn't infer it */
  stage?: Record<string, string>;
  /** a surprise the story means (a reveal nothing set up) */
  surprise?: "meant";
  /** findings about this beat the story means, by check (\"rhythm\", \"mckee:complications\" for a breather the
   *  journey wants): they're left out of what checks and lenses report */
  meant?: string | string[];
  says?: Line<A>[];
  /** what's on screen, when it says more than the events' `does` */
  onScreen?: Maybe<string>;
  seconds?: number;
  /** the scene it belongs to: consecutive beats with one scene are its shots (a feature's telling is scenes of shots) */
  scene?: string;
};

/** A telling's beats. A beat may be a choice point over beats, `null` among the options meaning "leave it out", so a
 *  telling can sample optional and alternative beats; a whole telling may be a choice too (alternative orders). */
export type Telling<F extends string, A extends string, E extends string, S extends string> = (Beat<F, A, E, S> | Choice<Beat<F, A, E, S> | null>)[];

export type StorySpec<
  V extends string, F extends string, A extends string, E extends string, S extends string, T extends string,
> = {
  title?: string;
  premise?: Maybe<string>;
  /** the controlling idea and its counter (McKee): the value it's about and what the story argues */
  idea?: { value?: NoInfer<V>; side: string; counter?: string };
  values: Record<V, Value>;
  facts?: Record<F, string>;
  agents: Record<A, Agent<F>>;
  /** events in story order (the order things happen in the world) */
  world: Record<E, Event<V, F, A, E>>;
  /** each telling: one cut of the film, a trailer, a game's critical path */
  tellings: Record<T, Telling<F, A, E, S> | Choice<Telling<F, A, E, S>>>;
  /** the lenses it's read and checked through by default (lens.ts) */
  lenses?: string[];
};

/** A story: the spec with its kind, so a brief, a check or a composition can tell it from other data. */
export type Story<
  V extends string = string, F extends string = string, A extends string = string,
  E extends string = string, S extends string = string, T extends string = string,
> = StorySpec<V, F, A, E, S, T> & { kind: "story" };

/** Any story, whatever its keys: what systems, checks and lenses take. */
export type AnyStory = Story<string, string, string, string, string, string>;

/** A story (NAME.story.ts), its references checked by tsc. */
export function story<
  const V extends string, const F extends string = never, const A extends string = never,
  const E extends string = never, const S extends string = never, const T extends string = never,
>(s: StorySpec<V, F, A, E, S, T>): Story<V, F, A, E, S, T> {
  return { ...s, kind: "story" };
}

export const isStory = (x: unknown): x is AnyStory => typeof x === "object" && x !== null && (x as { kind?: unknown }).kind === "story";

/** The causes of an event as a list. */
export const causesOf = (e: { because?: string | string[] }): string[] => (e.because === undefined ? [] : Array.isArray(e.because) ? e.because : [e.because]);
/** A setup or payoff field as a list. */
export const listOf = <T>(x: T | T[] | undefined): T[] => (x === undefined ? [] : Array.isArray(x) ? x : [x]);
