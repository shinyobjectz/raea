import { describe, expect, test } from "bun:test"
import { animStyle, apply, type Clip, empty, find, seqEdits, type Timeline } from "../src"

const film = (): Timeline => ({
  ...empty(30),
  tracks: [{ id: "main", kind: "main", clips: [{ id: "a", kind: "video", start: 0, length: 60, in: 0, source: 300 }, { id: "b", kind: "video", start: 60, length: 60, in: 60, source: 300 }] }],
})
const done = (t: Timeline, ...ops: Parameters<typeof apply>[1][]) =>
  ops.reduce((x, op) => {
    const r = apply(x, op)
    if (!r.ok) throw new Error(r.why)
    return r.timeline
  }, t)
const clip = (t: Timeline, id: string) => find(t, id)!.clip

describe("TL7d: a clip's own In and Out animations", () => {
  test("set on a clip, never longer than it, and taken off with null", () => {
    const t = done(film(), { op: "set", id: "a", set: { enter: { kind: "zoom", length: 15 }, exit: { kind: "slide", length: 500 } } })
    expect(clip(t, "a").enter).toEqual({ kind: "zoom", length: 15 })
    expect(clip(t, "a").exit).toEqual({ kind: "slide", length: 60 })
    const off = done(t, { op: "set", id: "a", set: { enter: null } })
    expect("enter" in clip(off, "a")).toBe(false)
    expect(clip(off, "a").exit).toEqual({ kind: "slide", length: 60 })
    expect(apply(t, { op: "set", id: "a", set: { enter: { kind: "", length: 10 } } })).toEqual({ ok: false, why: "an animation is a kind and at least one frame" })
    expect(apply(t, { op: "set", id: "a", set: { enter: { kind: "fade", length: 0 } } })).toEqual({ ok: false, why: "an animation is a kind and at least one frame" })
  })

  test("split, the first part keeps the In and the second the Out", () => {
    const t = done(film(), { op: "set", id: "a", set: { enter: { kind: "fade", length: 10 }, exit: { kind: "wipe", length: 10 } } }, { op: "split", at: 30, ids: ["a"] })
    expect([clip(t, "a").enter, clip(t, "a").exit]).toEqual([{ kind: "fade", length: 10 }, undefined])
    expect([clip(t, "a+").enter, clip(t, "a+").exit]).toEqual([undefined, { kind: "wipe", length: 10 }])
  })

  test("the picture's style over its first and last frames, as <Transition> draws it", () => {
    const c: Clip = { id: "a", kind: "video", start: 0, length: 60, enter: { kind: "fade", length: 10 }, exit: { kind: "zoom", length: 20 } }
    expect(animStyle(c, 4)).toEqual({ opacity: 0.5 })
    expect(animStyle(c, 30)).toEqual({})
    // into its Out: halfway through a zoom, scaled and faded
    const out = animStyle(c, 50)
    expect(out.opacity).toBeCloseTo(0.75)
    expect(Number(/^scale\(([\d.]+)\)$/.exec(out.transform ?? "")?.[1])).toBeCloseTo(0.9625)
    // a plugin's kind shows as a fade
    expect(animStyle({ ...c, enter: { kind: "glitch", length: 10 }, exit: undefined }, 4)).toEqual({ opacity: 0.5 })
  })

  test("in a composition each is its sequence's <Transition> edge, taken off with no frames", () => {
    const t: Timeline = { ...empty(30), tracks: [{ id: "main", kind: "main", clips: [{ id: "seq-0", kind: "sequence", start: 0, length: 60 }] }] }
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { enter: { kind: "slide", length: 12 } } })).toEqual([{ op: "edge", i: 0, side: "in", kind: "slide", frames: 12 }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { exit: null } })).toEqual([{ op: "edge", i: 0, side: "out", kind: "fade", frames: 0 }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { enter: { kind: "glitch", length: 12 } } })).toEqual({ why: 'a composition draws fade, slide, wipe, zoom: "glitch" isn\'t one its <Transition> has' })
  })
})
