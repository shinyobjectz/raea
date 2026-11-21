// Every change to a plan, as data (docs/plan.feature PL3): add, set, state, move, remove and check, each naming its
// task by :ID: (or #N for a headline that has none yet). applyEdits reads the text, makes the edits in order, gives
// every task an :ID: it lacks, brings progress cookies up to date and writes the text back; what no edit touched is
// as it was, byte for byte. The page, the host's tools and an agent's own script all change a plan this way.
import { plans, STATES, type PlanRegistry } from "./extend.ts"
import { isClosed, readPlan, type Plan, type State } from "./model.ts"
import { bare, checksOf, COOKIE, fileSequences, lineOf, printOrg, setCheck, type OrgDoc, type Planning, type Section } from "./org.ts"
import { dayOf, nowStamp } from "./time.ts"

export interface TaskFields {
  title?: string
  /** A, B or C (any capital or digit); null takes it off */
  priority?: string | null
  /** replaces its tags */
  tags?: string[]
  /** YYYY-MM-DD, MM-DD or an Org timestamp; null takes it off */
  deadline?: string | null
  scheduled?: string | null
  /** the feed item it stands on */
  item?: string | null
  /** who has it, by agent id; [] takes them off */
  agents?: string[]
  from?: string | null
  kind?: string | null
  verb?: string | null
  /** any other property; null takes it off */
  props?: Record<string, string | null>
  /** a line of text added under it */
  note?: string
}

export type PlanEdit =
  | ({ op: "add"; title: string; id?: string; state?: string; under?: string; before?: string; after?: string; checks?: string[] } & TaskFields)
  | ({ op: "set"; ref: string } & TaskFields)
  | { op: "state"; ref: string; state: string; item?: string; note?: string }
  | { op: "move"; ref: string; before?: string; after?: string; under?: string }
  | { op: "remove"; ref: string }
  | { op: "check"; ref: string; index?: number; done?: boolean; add?: string }

export interface ApplyOptions {
  now?: Date
  newId?: (taken: ReadonlySet<string>) => string
  registry?: PlanRegistry
}

export interface Applied {
  text: string
  /** each edit's task by its :ID: (an add's new id; a removed task's id as it was) */
  refs: string[]
  plan: Plan
}

/** A new task id: t- and eight letters and digits, unused in `taken`. */
export function newId(taken: ReadonlySet<string> = new Set()): string {
  for (;;) {
    const id = `t-${(Date.now() % 1e9).toString(36).padStart(6, "0")}${Math.floor(Math.random() * 1296).toString(36).padStart(2, "0")}`
    if (!taken.has(id)) return id
  }
}

/** A tag as Org reads one: letters, digits, _ @ # %. Anything else (a dash, a space) becomes _. */
export const tagOf = (s: string) => s.trim().replace(/^[#:]+/, "").replace(/[^\p{L}\p{N}_@#%]+/gu, "_").replace(/^_+|_+$/g, "")

/** The edit a checkbox makes: a closed task opens again, an open one is done. */
export const toggled = (t: { ref: string; state?: State }): PlanEdit => ({ op: "state", ref: t.ref, state: isClosed(t.state) ? "todo" : "done" })

/** A new project's plan: its title, and raea's keywords spelled out for Emacs and any other Org reader. */
/** A new plan.org's text. `keywords` is its #+TODO: sequence: raea's whole one unless the caller keeps fewer (the app
 *  keeps TODO | DONE DROPPED: where a linked task stands is its item's, PL8). */
export const newPlan = (title: string, keywords = "TODO MAKING REVIEW | DONE DROPPED") => `#+TITLE: ${title.replace(/\s+/g, " ").trim()}\n#+TODO: ${keywords}\n\n`

const oneLine = (s: string) => s.replace(/\s+/g, " ").trim()
const propOf = (s: Section, key: string) => s.props?.find(([k]) => k.toUpperCase() === key)?.[1]

function setProp(s: Section, key: string, value: string | null | undefined) {
  const k = key.toUpperCase()
  if (!/^[A-Z][\w-]*$/.test(k)) throw new Error(`a property is one word: ${key}`)
  const at = s.props?.findIndex(([x]) => x.toUpperCase() === k) ?? -1
  if (value === null || value === undefined || value === "") {
    if (at >= 0) s.props = s.props!.filter((_, i) => i !== at)
    if (s.props && !s.props.length) s.props = undefined
    return
  }
  const v = oneLine(value)
  if (at >= 0) s.props = s.props!.map((p, i) => (i === at ? [p[0], v] : p))
  else s.props = [...(s.props ?? []), [k, v]]
}

/** A line added to an entry's own text, before the blank lines that end it. */
function addLine(doc: OrgDoc, s: Section, text: string, after?: number) {
  let at = after ?? s.body.length
  if (after === undefined) while (at > 0 && !bare(s.body[at - 1]).trim()) at--
  s.body = [...s.body.slice(0, at), lineOf(doc, text), ...s.body.slice(at)]
}

interface Ctx {
  doc: OrgDoc
  words: Map<string, State>
  now: Date
  reg: PlanRegistry
  mint: () => string
  /** the headlines as they were before any edit, for #N */
  initial: Section[]
}

/** The keyword for a state or a keyword: a plan's own #+TODO: words first, then raea's. */
function wordFor(c: Ctx, want: string): string {
  if (c.words.has(want)) return want
  if (c.words.has(want.toUpperCase())) return want.toUpperCase()
  const state = want.toLowerCase() as State
  if (!STATES.includes(state)) throw new Error(`${want} is neither a keyword nor a state: ${[...c.words.keys()].join(" ")}, or ${STATES.join(", ")}`)
  for (const s of fileSequences(c.doc.head)) for (const w of [...s.open, ...s.done]) if (c.words.get(w) === state) return w
  for (const [w, st] of c.words) if (st === state) return w
  throw new Error(`no keyword in this plan means ${state}`)
}

/** The headline a ref names: an :ID:, or #N as the plan was before these edits. */
function find(c: Ctx, ref: string): Section | undefined {
  const n = /^#(\d+)$/.exec(ref)
  if (n) {
    const s = c.initial[Number(n[1])]
    return s && c.doc.sections.includes(s) ? s : undefined
  }
  return c.doc.sections.find((s) => propOf(s, "ID") === ref)
}

function must(c: Ctx, ref: string): Section {
  const s = find(c, ref)
  if (!s) throw new Error(`no task ${ref} in the plan; read the plan for its tasks' ids`)
  return s
}

/** A heading by its words (no keyword), made at the end of the plan when there's none. */
function heading(c: Ctx, title: string): Section {
  const words = oneLine(title).toLowerCase()
  const had = c.doc.sections.find((s) => !s.keyword && oneLine(s.title).toLowerCase() === words)
  if (had) return had
  const s: Section = { level: 1, title: oneLine(title), tags: [], planning: {}, body: [] }
  c.doc.sections.push(s)
  return s
}

/** Where a headline's subtree ends: the next headline at its level or above. */
function subtreeEnd(doc: OrgDoc, at: number): number {
  const level = doc.sections[at].level
  let end = at + 1
  while (end < doc.sections.length && doc.sections[end].level > level) end++
  return end
}

function fields(c: Ctx, s: Section, f: TaskFields) {
  if (f.title !== undefined) {
    const t = oneLine(f.title)
    if (!t) throw new Error("a task needs words")
    s.title = t
  }
  if (f.priority !== undefined) {
    const p = f.priority?.trim().toUpperCase()
    if (p && !/^[A-Z0-9]$/.test(p)) throw new Error(`a priority is one letter or digit (A, B, C): ${f.priority}`)
    s.priority = p || undefined
  }
  if (f.tags) s.tags = [...new Set(f.tags.map(tagOf).filter(Boolean))]
  for (const k of ["deadline", "scheduled"] as const) {
    const v = f[k]
    if (v === undefined) continue
    const { [k]: _, ...rest } = withoutRaw(s.planning, k)
    if (v === null || v === "") {
      s.planning = rest
      continue
    }
    const t = dayOf(v, c.now)
    if (!t) throw new Error(`${k}: a day as YYYY-MM-DD (or MM-DD this year): ${v}`)
    s.planning = { ...rest, [k]: t }
  }
  if (f.item !== undefined) setProp(s, "ITEM", f.item)
  if (f.agents) setProp(s, "AGENT", [...new Set(f.agents.map((a) => a.trim()).filter(Boolean))].join(" "))
  if (f.from !== undefined) setProp(s, "FROM", f.from)
  if (f.kind !== undefined) setProp(s, "KIND", f.kind)
  if (f.verb !== undefined) setProp(s, "VERB", f.verb)
  for (const [k, v] of Object.entries(f.props ?? {})) setProp(s, k, v)
  if (f.note?.trim()) addLine(c.doc, s, oneLine(f.note))
}

/** A planning line without the parts of one key that raea kept as written: a value set for that key replaces them. */
function withoutRaw(p: Planning, key: keyof Omit<Planning, "rest">): Planning {
  const rest = p.rest?.filter((r) => !r.toLowerCase().startsWith(`${key}:`))
  const { rest: _, ...out } = p
  return rest?.length ? { ...out, rest } : out
}

/** The file's #+TODO: lines name `word`'s sequence, so Emacs cycles it: a line is added after the head's last
 *  #+TODO: line (or its #+ lines) when a plugin's word is written into a file that doesn't name it. TODO and DONE, Org's
 *  own, need none in a file with no #+TODO: line. */
function declare(c: Ctx, word: string) {
  const seqs = fileSequences(c.doc.head)
  if (seqs.some((s) => s.open.includes(word) || s.done.includes(word))) return
  if (!seqs.length && (word === "TODO" || word === "DONE")) return
  const seq = [...c.reg.sequences.values()].find((s) => s.words.some((w) => w.word === word))
  if (!seq) return
  const open = seq.words.filter((w) => !isClosed(w.state)).map((w) => w.word)
  const done = seq.words.filter((w) => isClosed(w.state)).map((w) => w.word)
  const head = c.doc.head
  let at = 0
  head.forEach((l, i) => /^#\+(?:SEQ_|TYP_)?TODO:/i.test(bare(l)) && (at = i + 1))
  if (!at) while (at < head.length && bare(head[at]).startsWith("#+")) at++
  c.doc.head = [...head.slice(0, at), lineOf(c.doc, `#+TODO: ${open.join(" ")} | ${done.join(" ")}`), ...head.slice(at)]
}

/** A task's state set: closing it stamps CLOSED, reopening it takes CLOSED off (Org's org-log-done time). */
function mark(c: Ctx, s: Section, want: string) {
  const to = wordFor(c, want)
  const was = s.keyword ? c.words.get(s.keyword) : undefined
  s.keyword = to
  declare(c, to)
  const now = c.words.get(to)
  if (isClosed(now) && !isClosed(was)) s.planning = { ...withoutRaw(s.planning, "closed"), closed: nowStamp(c.now) }
  if (!isClosed(now)) {
    const { closed: _, ...rest } = withoutRaw(s.planning, "closed")
    s.planning = rest
  }
}

/** Where a block of headlines goes, and at what level: before or after a task, or last under a parent or heading. */
function place(c: Ctx, e: { before?: string; after?: string; under?: string }, level: number): { at: number; level: number } {
  if (e.before) {
    const t = must(c, e.before)
    return { at: c.doc.sections.indexOf(t), level: t.level }
  }
  if (e.after) {
    const t = must(c, e.after)
    return { at: subtreeEnd(c.doc, c.doc.sections.indexOf(t)), level: t.level }
  }
  if (e.under) {
    // a task's id that isn't there is a mistake, not a stage to make
    const p = find(c, e.under) ?? (/^(?:t-[a-z0-9]+|#\d+)$/i.test(e.under.trim()) ? must(c, e.under.trim()) : heading(c, e.under))
    return { at: subtreeEnd(c.doc, c.doc.sections.indexOf(p)), level: p.level + 1 }
  }
  return { at: c.doc.sections.length, level }
}

function one(c: Ctx, e: PlanEdit): Section | string {
  switch (e.op) {
    case "add": {
      const kind = e.kind ? c.reg.kinds.get(e.kind) : undefined
      const f: TaskFields = { ...kind?.defaults, ...e }
      const id = e.id?.trim() || c.mint()
      if (find(c, id)) throw new Error(`a task already has the id ${id}`)
      const s: Section = { level: 1, title: "", tags: [], planning: {}, props: [["ID", id]], body: [] }
      fields(c, s, { ...f, title: e.title })
      // a kind with a sequence of its own starts at that sequence's first word
      const first = kind?.keywords ? c.reg.sequences.get(kind.keywords)?.words[0]?.word : undefined
      mark(c, s, e.state ?? first ?? "todo")
      for (const check of e.checks ?? []) addLine(c.doc, s, `- [ ] ${oneLine(check)}`)
      const { at, level } = place(c, e, 1)
      s.level = level
      c.doc.sections.splice(at, 0, s)
      return s
    }
    case "set": {
      const s = must(c, e.ref)
      fields(c, s, e)
      return s
    }
    case "state": {
      const s = must(c, e.ref)
      mark(c, s, e.state)
      fields(c, s, { item: e.item, note: e.note })
      return s
    }
    case "move": {
      const s = must(c, e.ref)
      if (!e.before && !e.after && !e.under) throw new Error("a move needs before, after or under")
      const from = c.doc.sections.indexOf(s)
      const block = c.doc.sections.slice(from, subtreeEnd(c.doc, from))
      for (const r of [e.before, e.after, e.under]) if (r && block.some((b) => b === find(c, r))) throw new Error("a task can't move into itself")
      c.doc.sections.splice(from, block.length)
      const { at, level } = place(c, e, s.level)
      const delta = level - s.level
      for (const b of block) b.level = Math.max(1, b.level + delta)
      c.doc.sections.splice(at, 0, ...block)
      return s
    }
    case "remove": {
      const s = must(c, e.ref)
      const from = c.doc.sections.indexOf(s)
      c.doc.sections.splice(from, subtreeEnd(c.doc, from) - from)
      return propOf(s, "ID") ?? e.ref
    }
    case "check": {
      const s = must(c, e.ref)
      const checks = checksOf(s.body)
      if (e.add?.trim()) {
        const last = checks.at(-1)
        const indent = last ? bare(s.body[last.line]).match(/^[ \t]*/)![0] : ""
        addLine(c.doc, s, `${indent}- [ ] ${oneLine(e.add)}`, last ? last.line + 1 : undefined)
        return s
      }
      const ch = e.index === undefined ? undefined : checks[e.index]
      if (!ch) throw new Error(`no checkbox ${e.index} under ${e.ref}: it has ${checks.length}`)
      s.body = s.body.map((l, i) => (i === ch.line ? setCheck(l, e.done ?? !ch.done) : l))
      return s
    }
  }
}

/** Each [n/m] or [n%] cookie counts again: the tasks right under its headline, or, with none, its checkboxes. */
function cookies(c: Ctx) {
  const parents: number[] = []
  const stack: number[] = []
  c.doc.sections.forEach((s, i) => {
    while (stack.length && c.doc.sections[stack.at(-1)!].level >= s.level) stack.pop()
    parents[i] = stack.at(-1) ?? -1
    stack.push(i)
  })
  c.doc.sections.forEach((s, i) => {
    const m = COOKIE.exec(s.title)
    if (!m) return
    const kids = c.doc.sections.filter((k, j) => parents[j] === i && k.keyword)
    let done: number
    let total: number
    if (kids.length) {
      done = kids.filter((k) => isClosed(c.words.get(k.keyword!))).length
      total = kids.length
    } else {
      const checks = checksOf(s.body)
      const top = Math.min(...checks.map((x) => x.indent))
      const own = checks.filter((x) => x.indent === top)
      done = own.filter((x) => x.done).length
      total = own.length
    }
    const cookie = m[3] !== undefined ? `[${total ? Math.round((100 * done) / total) : 0}%]` : `[${done}/${total}]`
    if (cookie !== m[0]) s.title = s.title.replace(COOKIE, cookie)
  })
}

/** Makes the edits on the plan's text, in order, and answers with the new text. Throws, changing nothing, on an edit
 *  that names no task or a value that can't be written. */
export function applyEdits(text: string, edits: readonly PlanEdit[], o: ApplyOptions = {}): Applied {
  const reg = o.registry ?? plans
  const before = readPlan(text, reg)
  const taken = new Set(before.entries.flatMap((t) => (t.id ? [t.id] : [])))
  const c: Ctx = {
    doc: before.doc,
    words: before.words,
    now: o.now ?? new Date(),
    reg,
    initial: [...before.doc.sections],
    mint: () => {
      const id = (o.newId ?? newId)(taken)
      taken.add(id)
      return id
    },
  }
  const done = edits.map((e) => one(c, e))
  for (const s of c.doc.sections) if (s.keyword && !propOf(s, "ID")) setProp(s, "ID", c.mint())
  cookies(c)
  const out = printOrg(c.doc)
  return { text: out, refs: done.map((d) => (typeof d === "string" ? d : (propOf(d, "ID") ?? ""))), plan: readPlan(out, reg) }
}
