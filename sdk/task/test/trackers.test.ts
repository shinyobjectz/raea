// PL8: a task kept in step with another tracker by :LINK: and #+LINK_STATES:, with checkPlan saying what's wrong, and
// a raea cloud plan.org (the cloud's writer, 11-20) passing it with nothing flagged.
import { describe, expect, test } from "vitest"
import { checkPlan, keywordFor, newPlan, readPlan, statusFor, trackersOf, trackerStates } from "../src/index.ts"

const LINKED = `#+TITLE: Launch
#+TODO: TODO MAKING REVIEW | DONE DROPPED
#+LINK_STATES: linear In Progress=MAKING; In Review=REVIEW; Done=DONE; Canceled=DROPPED; Duplicate=DROPPED

* Cut
** TODO Hero cut
:PROPERTIES:
:ID:       t-hero
:LINK:     linear:ENG-12 asana:1209876543
:END:
`

describe("PL8: links to other trackers", () => {
  test("a task's trackers are its :LINK: entries, in order", () => {
    const p = readPlan(LINKED)
    expect(trackersOf(p.tasks[0])).toEqual([{ system: "linear", id: "ENG-12" }, { system: "asana", id: "1209876543" }])
    // each tracker's own key, as raea cloud writes them (11-20): GitHub's org/repo#12, Slack's channel/ts, Trello's shortLink
    const keys = readPlan(LINKED.replace("linear:ENG-12 asana:1209876543", "github:acme/site#12 slack:C0123/1700000000.123 trello:aB3dE9 jira:PROJ-7"))
    expect(trackersOf(keys.tasks[0]).map((t) => t.id)).toEqual(["acme/site#12", "C0123/1700000000.123", "aB3dE9", "PROJ-7"])
    expect(checkPlan(keys)).toEqual([])
  })

  test("a status comes in as its keyword, and a keyword goes out as the first status listed for it", () => {
    const p = readPlan(LINKED)
    expect(keywordFor(p, "linear", " in review ")).toBe("REVIEW")
    expect(keywordFor(p, "Linear", "Duplicate")).toBe("DROPPED")
    expect(statusFor(p, "linear", "DROPPED")).toBe("Canceled")
    expect(keywordFor(p, "linear", "Backlog")).toBeUndefined()
    expect(keywordFor(p, "jira", "Done")).toBeUndefined()
    expect([...trackerStates(p).keys()]).toEqual(["linear"])
    expect(checkPlan(p)).toEqual([])
  })

  test("checkPlan says what's wrong with a link, and how to fix it", () => {
    const bad = `${LINKED.replace("asana:1209876543", "linear:ENG-13 https://linear.app/x ENG-14")}** TODO Same item
:PROPERTIES:
:LINK:     linear:ENG-12
:END:
`.replace("Duplicate=DROPPED", "Blocked=WAITING")
    const says = checkPlan(readPlan(bad)).map((d) => d.message)
    expect(says.some((m) => /names linear twice/.test(m))).toBe(true)
    expect(says.some((m) => /is a URL/.test(m))).toBe(true)
    expect(says.some((m) => /"ENG-14" isn't SYSTEM:ID/.test(m))).toBe(true)
    expect(says.some((m) => /linear:ENG-12 is also Hero cut's/.test(m))).toBe(true)
    expect(says.some((m) => /WAITING isn't one of this plan's keywords/.test(m))).toBe(true)
  })

  test("a new plan keeps the keywords it's given", () => {
    expect(newPlan("Pong")).toContain("#+TODO: TODO MAKING REVIEW | DONE DROPPED")
    expect(newPlan("Pong", "TODO | DONE DROPPED")).toContain("#+TODO: TODO | DONE DROPPED\n")
  })
})

// what raea cloud writes (11-20): an * Agent heading, triggers under * Triggers (and the old * Inputs),
// projects with their properties, a project's own sections, checkpoints, team stage checklists and logbook notes
const CLOUD = `#+TITLE: Cutter
#+TODO: TODO MAKING REVIEW | DONE DROPPED
#+LINK_STATES: linear In Progress=MAKING; In Review=REVIEW; Done=DONE

* Agent
:PROPERTIES:
:FOR:      Acme's paid social
:AT_A_TIME: no limit
:TOOLS:    computer judge web
:END:
- [X] Client review (amber): where its review goes
- [ ] Legal (red): legal@acme.com

* Triggers
** Linear: In Review on ENG
:PROPERTIES:
:TRIGGER:  tr-lin1
:APP:      linear
:WHERE:    ENG
:SIGNAL:   status
:VALUE:    In Review
:FIELD:    state
:FILTERS:  label=video; priority=high
:REPLY:    webhook https://hooks.acme.com/raea
:END:
** Every Monday
:PROPERTIES:
:TRIGGER:  tr-sched
:AT:       09:00
:REPEAT:   weekly
:END:

* Inputs
** Old form
:PROPERTIES:
:INPUT:    in-1
:END:

* Projects
** REVIEW Spring sale cut [1/2]
:PROPERTIES:
:ID:       pj-1
:AGENT:    ag-cutter
:BOT:      bot-1
:SKILL:    sk-cut
:REPO:     project-pj-1
:ASSIGNEE: email:ana@acme.com Ana Ruiz
:FROM:     tr-lin1
:REPLY:    email ana@acme.com
:CLIENT:   Acme
:BRAND:    Acme
:CREATED:  [2026-11-20 Fri 10:02]
:SPENT:    0.039
:RESPONSES: 3
:WAITING:  Client review
:LINK:     linear:ENG-12
:END:
:LOGBOOK:
- Note taken on [2026-11-20 Fri 10:10] \\\\
  first cut posted
:END:
- [X] Approve the plan
- [ ] Client review: Review the first cut
*** Input
[[https://linear.app/acme/issue/ENG-12][ENG-12]]
*** Attachments
*** Expected output
*** Checkpoint: Approve the plan
:PROPERTIES:
:AT:       [2026-11-20 Fri 10:05]
:ANSWER:   approve
:NOTE:     go
:END:
** DROPPED Old idea
:PROPERTIES:
:ID:       pj-0
:OUTCOME:  failed
:END:
*** What went wrong
*** Last answer
*** Delivered
`

test("PL8: a raea cloud plan.org passes checkPlan with nothing flagged", () => {
  const p = readPlan(CLOUD)
  expect(checkPlan(p)).toEqual([])
  expect(p.tasks.map((t) => t.keyword)).toEqual(["REVIEW", "DROPPED"])
  expect(trackersOf(p.tasks[0])).toEqual([{ system: "linear", id: "ENG-12" }])
})
