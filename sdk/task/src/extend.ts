// What a plugin adds to plans (docs/plan.feature PL6): TODO keyword sequences, properties with a check, kinds of task
// with the properties they need and a check of their own, and views (an agenda, board lanes, anything built from a
// plan). A registry holds them; `plans` is the one everything uses unless it's handed another. raea's own sequence
// (TODO MAKING REVIEW | DONE DROPPED), its properties and its views are in every registry from the start.
import type { TaskFields } from "./edit.ts"
import type { Plan, State, Task } from "./model.ts"
import { trackerProblems } from "./trackers.ts"
import { agenda, board, outline } from "./views.ts"

export const STATES: readonly State[] = ["todo", "making", "review", "done", "dropped"]

export interface KeywordSequence {
  name: string
  words: { word: string; state: State }[]
}

export interface PropertyDef {
  /** as written in the drawer, upper case: SHOT, not :shot: */
  key: string
  about?: string
  /** what is wrong with a value, or undefined when it's fine */
  check?: (value: string) => string | undefined
}

export interface PlanKind {
  /** what :KIND: says */
  name: string
  about?: string
  /** the keyword sequence its tasks move through, by name */
  keywords?: string
  /** the properties a task of this kind must have */
  props?: string[]
  /** what is wrong with a task of this kind, or undefined */
  check?: (task: Task, plan: Plan) => string | undefined
  /** what a new task of this kind starts with (plan edits read it when an add names the kind) */
  defaults?: Partial<TaskFields>
}

export interface PlanView<T = unknown, O = never> {
  name: string
  about?: string
  build: (plan: Plan, options?: O) => T
}

export interface PlanRegistry {
  sequences: Map<string, KeywordSequence>
  /** every keyword any sequence has, with its state; the first sequence to name a word decides it */
  words: Map<string, State>
  props: Map<string, PropertyDef>
  kinds: Map<string, PlanKind>
  views: Map<string, PlanView<unknown, never>>
}

/** A sequence's states, Org's way when a word's state isn't given or known: before the bar, the first word is to do
 *  and the rest are under way; after it, the first is done and the rest dropped. */
export function sequence(name: string, open: string[], done: string[], states: Record<string, State> = {}, known?: ReadonlyMap<string, State>): KeywordSequence {
  const pick = (word: string, fallback: State): State => states[word] ?? known?.get(word) ?? fallback
  return { name, words: [...open.map((word, i) => ({ word, state: pick(word, i === 0 ? "todo" : "making") })), ...done.map((word, i) => ({ word, state: pick(word, i === 0 ? "done" : "dropped") }))] }
}

/** A sequence written the way a #+TODO: line writes it: "DRAFT CUT GRADE | DELIVERED". */
export function defineKeywords(def: { name: string; words: string; states?: Record<string, State> }, reg: PlanRegistry = plans): KeywordSequence {
  const words = def.words.trim().split(/\s+/)
  const bar = words.indexOf("|")
  const open = bar >= 0 ? words.slice(0, bar) : words.slice(0, -1)
  const done = bar >= 0 ? words.slice(bar + 1) : words.slice(-1)
  for (const w of [...open, ...done]) if (!/^[A-Z][A-Z0-9_-]*$/.test(w)) throw new Error(`a TODO keyword is one word in capitals: ${w}`)
  const seq = sequence(def.name, open, done, def.states, reg.words)
  reg.sequences.set(seq.name, seq)
  for (const { word, state } of seq.words) if (!reg.words.has(word)) reg.words.set(word, state)
  return seq
}

export function defineProperty(def: PropertyDef, reg: PlanRegistry = plans): PropertyDef {
  const key = def.key.toUpperCase()
  if (!/^[A-Z][A-Z0-9_-]*$/.test(key)) throw new Error(`a property is one word: ${def.key}`)
  const d = { ...def, key }
  reg.props.set(key, d)
  return d
}

export function definePlanKind(def: PlanKind, reg: PlanRegistry = plans): PlanKind {
  if (!/^[a-z][\w-]*$/.test(def.name)) throw new Error(`a kind is one lower-case word: ${def.name}`)
  if (def.keywords && !reg.sequences.has(def.keywords)) throw new Error(`no keyword sequence ${def.keywords}; defineKeywords it first`)
  reg.kinds.set(def.name, def)
  return def
}

export function defineView<T, O = never>(def: PlanView<T, O>, reg: PlanRegistry = plans): PlanView<T, O> {
  reg.views.set(def.name, def as unknown as PlanView<unknown, never>)
  return def
}

/** A view of a plan by name: what its build makes. */
export function view<T = unknown, O = never>(plan: Plan, name: string, options?: O, reg: PlanRegistry = plans): T {
  const v = reg.views.get(name)
  if (!v) throw new Error(`no plan view ${name}: ${[...reg.views.keys()].join(", ")}`)
  return (v.build as (p: Plan, o?: O) => T)(plan, options)
}

/** A registry with raea's own sequence, properties and views. */
export function createRegistry(): PlanRegistry {
  const reg: PlanRegistry = { sequences: new Map(), words: new Map(), props: new Map(), kinds: new Map(), views: new Map() }
  // only raea's words: a plain heading such as "* NEXT steps" stays a heading unless the file's #+TODO: names NEXT
  defineKeywords({ name: "raea", words: "TODO MAKING REVIEW | DONE DROPPED", states: { REVIEW: "review" } }, reg)
  const id = (v: string) => (/^[\w.:#@-]+$/.test(v) ? undefined : "an id is letters, digits and . _ - : # @, no spaces")
  defineProperty({ key: "ID", about: "the task's id, which never changes: everything links to a task by it", check: id }, reg)
  defineProperty({ key: "ITEM", about: "the feed item the task stands on, by id: what was made", check: id }, reg)
  defineProperty({ key: "AGENT", about: "who has it: agent ids, space-separated (an @name only until an agent has that name)" }, reg)
  defineProperty({ key: "FROM", about: "what asked for it: a feed message's id, or ITEM#COMMENT for a comment" }, reg)
  defineProperty({ key: "KIND", about: "the task's kind (definePlanKind)" }, reg)
  defineProperty({ key: "VERB", about: "the craft verb it asks for (polish, recut, regrade ...)" }, reg)
  defineView({ name: "board", about: "lanes by state: to do, making, to review, done", build: board }, reg)
  defineView({ name: "agenda", about: "what's due or scheduled, day by day, and what's overdue", build: agenda }, reg)
  defineView({ name: "outline", about: "the plan as a tree", build: outline }, reg)
  return reg
}

/** The registry everything uses unless handed another. */
export const plans: PlanRegistry = createRegistry()

export interface Diagnostic {
  ref: string
  /** 0-based line of the headline in the file */
  line: number
  severity: "error" | "warning"
  message: string
}

/** What is wrong in a plan by the registry's rules: ids used twice, properties that fail their check, kinds nobody
 *  defined, a kind's missing properties, keywords outside a kind's sequence, the kinds' own checks, and its links to
 *  other trackers (trackers.ts: an entry that isn't SYSTEM:ID, one tracker item on two tasks, a #+LINK_STATES:
 *  keyword the plan doesn't know). Nothing else is required: no :ID:, no keyword on a heading, no property. */
export function checkPlan(plan: Plan, reg: PlanRegistry = plans): Diagnostic[] {
  const out: Diagnostic[] = []
  const seen = new Map<string, Task>()
  for (const t of plan.entries) {
    const say = (message: string, severity: Diagnostic["severity"] = "warning") => out.push({ ref: t.ref, line: t.line, severity, message })
    if (t.id) {
      if (seen.has(t.id)) say(`:ID: ${t.id} is also ${seen.get(t.id)!.text}'s; an id is one task's`, "error")
      else seen.set(t.id, t)
    }
    for (const [k, v] of Object.entries(t.props)) {
      const why = reg.props.get(k)?.check?.(v)
      if (why) say(`:${k}: ${why}`)
    }
    if (!t.kind) continue
    const kind = reg.kinds.get(t.kind)
    if (!kind) {
      say(`no kind ${t.kind} is defined (${[...reg.kinds.keys()].join(", ") || "none yet"})`)
      continue
    }
    for (const p of kind.props ?? []) if (!(p.toUpperCase() in t.props)) say(`a ${kind.name} needs :${p.toUpperCase()}:`)
    const seq = kind.keywords ? reg.sequences.get(kind.keywords) : undefined
    if (seq && t.keyword && !seq.words.some((w) => w.word === t.keyword)) say(`a ${kind.name} moves through ${seq.words.map((w) => w.word).join(" ")}, not ${t.keyword}`)
    const why = kind.check?.(t, plan)
    if (why) say(why)
  }
  for (const p of trackerProblems(plan)) out.push({ ref: p.task?.ref ?? "head", line: p.task?.line ?? p.line ?? 0, severity: "error", message: p.message })
  return out
}
