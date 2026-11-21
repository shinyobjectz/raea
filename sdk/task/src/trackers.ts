// A task kept in step with another tracker (Asana, Linear, Jira, ...: raea cloud's board triggers, a client's own
// system), with no code per client: the file says it all (docs/plan.feature PL8).
//   :LINK: linear:ENG-12 asana:1209876543     on a task: one SYSTEM:ID per system, the tracker's own key, never a URL
//   #+LINK_STATES: linear In Progress=MAKING; In Review=REVIEW; Done=DONE     in the head: one line per system
// A status comes in as its keyword (trimmed, any case); a keyword goes out as the FIRST status the line lists for it.
import type { Plan, Task } from "./model.ts"

export interface Tracker {
  system: string
  id: string
}

export interface TrackerStates {
  /** a status (lower case, trimmed) to its keyword */
  toKeyword: Map<string, string>
  /** a keyword to the first status listed for it, as written */
  toStatus: Map<string, string>
}

const SYSTEM = /^[a-z0-9][a-z0-9-]*$/
const STATES_LINE = /^[ \t]*#\+LINK_STATES:[ \t]*(\S+)[ \t]*(.*)$/i

/** One :LINK: entry, or why it isn't one. */
function entry(s: string): Tracker | string {
  const at = s.indexOf(":")
  const system = at > 0 ? s.slice(0, at) : ""
  const id = at > 0 ? s.slice(at + 1) : ""
  if (!SYSTEM.test(system) || !id) return `"${s}" isn't SYSTEM:ID (linear:ENG-12)`
  if (/^\/\/|^https?:/i.test(id) || /^https?$/.test(system)) return `"${s}" is a URL; :LINK: takes the tracker's own key, the URL goes in the task's text`
  return { system, id }
}

/** The trackers a task is kept in step with, in the order its :LINK: names them (what isn't SYSTEM:ID is left out). */
export function trackersOf(task: Task): Tracker[] {
  return (task.props.LINK ?? "").split(/\s+/).filter(Boolean).map(entry).filter((e): e is Tracker => typeof e !== "string")
}

/** Each system's status map, from the head's #+LINK_STATES: lines. */
export function trackerStates(plan: Plan): Map<string, TrackerStates> {
  const out = new Map<string, TrackerStates>()
  for (const line of plan.doc.head) {
    const m = STATES_LINE.exec(line)
    if (!m) continue
    const system = m[1].toLowerCase()
    const s = out.get(system) ?? { toKeyword: new Map(), toStatus: new Map() }
    for (const pair of m[2].split(";")) {
      const eq = pair.lastIndexOf("=")
      const status = pair.slice(0, eq).trim()
      const keyword = pair.slice(eq + 1).trim()
      if (eq < 0 || !status || !keyword) continue
      s.toKeyword.set(status.toLowerCase(), keyword)
      if (!s.toStatus.has(keyword)) s.toStatus.set(keyword, status)
    }
    out.set(system, s)
  }
  return out
}

/** The keyword a system's status means here, or undefined when the head doesn't map it. */
export const keywordFor = (plan: Plan, system: string, status: string): string | undefined =>
  trackerStates(plan).get(system.toLowerCase())?.toKeyword.get(status.trim().toLowerCase())

/** The status to write back to a system for a keyword: the first one its line lists, or undefined. */
export const statusFor = (plan: Plan, system: string, keyword: string): string | undefined => trackerStates(plan).get(system.toLowerCase())?.toStatus.get(keyword)

/** What's wrong with the plan's links, for checkPlan: an entry that isn't SYSTEM:ID, a system named twice on one task,
 *  one SYSTEM:ID on two tasks, and a #+LINK_STATES: keyword the plan doesn't know. Each says its fix. */
export function trackerProblems(plan: Plan): { task?: Task; line?: number; message: string }[] {
  const out: { task?: Task; line?: number; message: string }[] = []
  const owner = new Map<string, Task>()
  for (const t of plan.entries) {
    const systems = new Set<string>()
    for (const raw of (t.props.LINK ?? "").split(/\s+/).filter(Boolean)) {
      const e = entry(raw)
      if (typeof e === "string") {
        out.push({ task: t, message: `:LINK: ${e}` })
        continue
      }
      if (systems.has(e.system)) out.push({ task: t, message: `:LINK: names ${e.system} twice; a task is one item in each system` })
      systems.add(e.system)
      const key = `${e.system}:${e.id}`
      const was = owner.get(key)
      if (was) out.push({ task: t, message: `:LINK: ${key} is also ${was.text}'s; one tracker item is one task, so keep it on one of them`, })
      else owner.set(key, t)
    }
  }
  plan.doc.head.forEach((line, i) => {
    const m = STATES_LINE.exec(line)
    if (!m) return
    if (!SYSTEM.test(m[1].toLowerCase())) out.push({ line: i, message: `#+LINK_STATES: starts with its system's name (linear), not "${m[1]}"` })
    for (const pair of m[2].split(";").map((p) => p.trim()).filter(Boolean)) {
      const eq = pair.lastIndexOf("=")
      const keyword = eq < 0 ? "" : pair.slice(eq + 1).trim()
      if (eq <= 0 || !keyword) out.push({ line: i, message: `#+LINK_STATES: "${pair}" isn't Status=KEYWORD` })
      else if (!plan.words.has(keyword)) out.push({ line: i, message: `#+LINK_STATES: ${keyword} isn't one of this plan's keywords (${[...plan.words.keys()].join(" ")}); add it to #+TODO: or map the status to one that is` })
    }
  })
  return out
}
