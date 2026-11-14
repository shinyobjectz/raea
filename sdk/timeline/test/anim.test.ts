import { describe, expect, test } from "bun:test"
import { animStyle, apply, type Clip, empty, find, fromSequences, seqEdits, type Timeline } from "../src"

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

  test("TL7e: a Combo over the whole clip, a cycle every so many frames, with its In and Out", () => {
    const t = done(film(), { op: "set", id: "a", set: { combo: { kind: "pulse", length: 60 }, enter: { kind: "fade", length: 10 } } })
    expect(clip(t, "a").combo).toEqual({ kind: "pulse", length: 60 })
    const c = clip(t, "a")
    expect(animStyle(c, 30)).toEqual({ transform: "scale(1.08)" })
    expect(animStyle(c, 4)).toEqual({ opacity: 0.5, transform: "scale(1.004)" })
    // a plugin's Combo draws from its own wave; one the palette doesn't have draws nothing
    const wobble = { kind: "wobble", title: "Wobble", wave: { x: [0, 10, 0] } }
    const w = done(t, { op: "set", id: "a", set: { combo: { kind: "wobble", length: 20 }, enter: null } })
    expect(animStyle(clip(w, "a"), 10, [wobble])).toEqual({ transform: "translate(10%, 0%)" })
    expect(animStyle(clip(w, "a"), 10)).toEqual({})
    // split, both parts keep it; null takes it off
    const s = done(t, { op: "split", at: 30, ids: ["a"] })
    expect([clip(s, "a").combo, clip(s, "a+").combo]).toEqual([{ kind: "pulse", length: 60 }, { kind: "pulse", length: 60 }])
    expect("combo" in clip(done(t, { op: "set", id: "a", set: { combo: null } }), "a")).toBe(false)
  })

  test("TL7e: in a composition it is the sequence's <Transition combo>, read back as the clip's", () => {
    const t: Timeline = { ...empty(30), tracks: [{ id: "main", kind: "main", clips: [{ id: "seq-0", kind: "sequence", start: 0, length: 60 }] }] }
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { combo: { kind: "sway", length: 30 } } })).toEqual([{ op: "edge", i: 0, side: "combo", kind: "sway", frames: 30 }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { combo: null } })).toEqual([{ op: "edge", i: 0, side: "combo", kind: "pulse", frames: 0 }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { combo: { kind: "wobble", length: 30 } } })).toEqual({ why: expect.stringContaining('"wobble" isn\'t one its <Transition> has') })
    const back = fromSequences([{ i: 0, name: "Sequence", from: 0, to: 60, lane: 0, combo: { kind: "sway", frames: 30 } }], 30)
    expect(back.tracks[0].clips[0].combo).toEqual({ kind: "sway", length: 30 })
  })

  test("read back from the code: an edge a cut's transition didn't take is the clip's own", () => {
    const row = (i: number, from: number, extra = {}) => ({ i, name: "Sequence", from, to: from + 60, lane: 0, ...extra })
    // A comes in by a zoom; A and B share a slide on their cut; B goes out by a wipe
    const t = fromSequences([row(0, 0, { in: { kind: "zoom", frames: 12 }, out: { kind: "slide", frames: 8 } }), row(1, 60, { in: { kind: "slide", frames: 7 }, out: { kind: "wipe", frames: 90 } })], 30)
    expect(t.transitions).toEqual([{ id: "x-seq-0", kind: "slide", after: "seq-0", length: 15 }])
    const [a, b] = t.tracks[0].clips
    expect([a.enter, a.exit, b.enter, b.exit]).toEqual([{ kind: "zoom", length: 12 }, undefined, undefined, { kind: "wipe", length: 60 }])
    // edges of different kinds on a cut are each clip's own
    const apart = fromSequences([row(0, 0, { out: { kind: "fade", frames: 8 } }), row(1, 60, { in: { kind: "zoom", frames: 7 } })], 30)
    expect(apart.transitions).toEqual([])
    expect([apart.tracks[0].clips[0].exit, apart.tracks[0].clips[1].enter]).toEqual([{ kind: "fade", length: 8 }, { kind: "zoom", length: 7 }])
  })
})
