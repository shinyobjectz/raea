import { describe, expect, test } from "bun:test"
import { COMBOS, CURVES, EASES, gridFrames, palette, readAdds, seqEdits, fromSequences, TEXTS, TRANSITIONS } from "../src"

describe("TL11a: a plugin adds easings, speed curves and transitions", () => {
  const raw = {
    eases: { snap: [0.9, 0, 0.1, 1], back: [0.3, 1.6, 0.6, 1], linear: "hold", bad: [2, 0, 0, 1] },
    curves: { pulse: [{ at: 0, speed: 1 }, { at: 0.5, speed: 4 }, { at: 1, speed: 1 }], hero: [{ at: 0, speed: 1 }, { at: 1, speed: 1 }], wild: [{ at: 0, speed: 50 }, { at: 1, speed: 1 }] },
    transitions: [{ kind: "Glitch", title: "Glitch" }, { kind: "fade", title: "Mine" }, { kind: "", title: "x" }],
    tracks: [{ kind: "lighting", title: "Lighting cues" }, { kind: "sound", title: "Mine" }, { kind: "Bad Kind", title: "x" }, { kind: "cues", title: "Cues", under: true }],
  }

  test("what is well formed is offered after raea's own; the rest is left out with why", () => {
    const { adds, errors } = readAdds(raw)
    expect(adds).toEqual({ eases: { snap: [0.9, 0, 0.1, 1], back: [0.3, 1.6, 0.6, 1] }, curves: { pulse: raw.curves.pulse }, transitions: [{ kind: "Glitch", title: "Glitch" }], tracks: [{ kind: "lighting", title: "Lighting cues" }, { kind: "cues", title: "Cues", under: true }] })
    expect(errors).toEqual([
      "timeline.eases.linear: raea has an easing of that name",
      'timeline.eases.bad is "linear", "hold" or a bezier\'s handles [x1, y1, x2, y2] with x1 and x2 from 0 to 1',
      "timeline.curves.hero: raea has a speed curve of that name",
      "timeline.curves.wild is two or more points { at, speed }, at from 0 to 1 and speed from 0.1 to 10",
      'timeline.transitions[1]: raea has a transition "fade"',
      "timeline.transitions[2] is { kind, title }: kind a name, title what the person sees",
      'timeline.tracks[1]: raea has a track of kind "sound"',
      "timeline.tracks[2] is { kind, title }: kind in lowercase letters, digits and dashes, title what the person sees",
    ])
    expect(palette([adds]).tracks.map((t) => t.kind)).toEqual(["effect", "text", "overlay", "main", "sound", "lighting", "cues"])
    const all = palette([adds, { eases: { snap: "hold" } }])
    expect(Object.keys(all.eases)).toEqual([...Object.keys(EASES), "snap", "back"])
    expect(all.eases.snap).toEqual([0.9, 0, 0.1, 1])
    expect(Object.keys(all.curves)).toEqual([...Object.keys(CURVES), "pulse"])
    expect(all.transitions.map((t) => t.kind)).toEqual([...TRANSITIONS.map((t) => t.kind), "Glitch"])
  })

  test("with no plugin it is raea's own, and a manifest with no timeline adds nothing", () => {
    expect(palette().transitions).toEqual(TRANSITIONS)
    expect(readAdds(undefined)).toEqual({ adds: {}, errors: [] })
    expect(readAdds([1]).errors).toEqual(["timeline is an object: { eases, curves, transitions, combos, tracks, effects, texts, tools, snaps }"])
  })

  test("TL7e: a plugin's Combos, each a wave of the props a clip moves by, checked", () => {
    const wobble = { kind: "wobble", title: "Wobble", wave: { x: [0, 4, 0], rotate: [0, 2, 0] } }
    const { adds, errors } = readAdds({ combos: [wobble, { kind: "pulse", title: "Mine", wave: { scale: [1, 2] } }, { kind: "bad", title: "Bad", wave: { skew: [1] } }, { kind: "empty", title: "Empty", wave: { x: [] } }] })
    expect(adds.combos).toEqual([wobble])
    expect(errors).toEqual(['timeline.combos[1]: raea has a combo "pulse"', "timeline.combos[2] (bad): a wave moves x, y, scale, rotate, opacity, not skew", "timeline.combos[3] (empty): wave.x is 1 to 64 numbers over one cycle"])
    expect(palette([adds]).combos.map((c) => c.kind)).toEqual([...COMBOS.map((c) => c.kind), "wobble"])
  })
})

describe("TL11a: a plugin's effects", () => {
  test("are data: CSS filter functions from amount 0 to amount 1, checked, and offered after raea's own", () => {
    const { adds, errors } = readAdds({
      effects: [
        { kind: "dream", title: "Dream", parts: [["blur", 0, 6, "px"], ["saturate", 1, 1.6]] },
        { kind: "blur", title: "Mine", parts: [["blur", 0, 2, "px"]] },
        { kind: "shadow", title: "Shadow", parts: [["drop-shadow", 0, 1]] },
        { kind: "Bad Name", title: "x", parts: [["blur", 0, 2, "px"]] },
      ],
    })
    expect(adds.effects).toEqual([{ kind: "dream", title: "Dream", parts: [["blur", 0, 6, "px"], ["saturate", 1, 1.6]] }])
    expect(errors).toEqual([
      'timeline.effects[1]: raea has an effect "blur"',
      expect.stringContaining("timeline.effects[2] (shadow): a part's function is one of"),
      "timeline.effects[3] is { kind, title, parts }: kind in lowercase letters, digits and dashes, title what the person sees",
    ])
    const all = palette([adds]).effects
    expect(all[all.length - 1]).toEqual(adds.effects![0])
    expect(all.length).toBe(11)
  })
})

describe("TL7c, TL11a: text styles", () => {
  test("raea's own, and a plugin's checked: type and colour only, nothing that reads as code", () => {
    const neon = { kind: "neon", title: "Neon", place: "center", style: { color: "#39ff14", fontSize: 96, textShadow: "0 0 24px #39ff14" } }
    const { adds, errors } = readAdds({ texts: [neon, { ...neon, kind: "title" }, { ...neon, kind: "wide", style: { width: "100%" } }, { ...neon, kind: "sly", style: { background: "url(http://x)" } }, { ...neon, kind: "low", place: "left" }] })
    expect(adds.texts).toEqual([neon])
    expect(errors).toEqual(['timeline.texts[1]: raea has a text style "title"', expect.stringContaining("timeline.texts[2] style.width: a text style sets"), "timeline.texts[3] style.background is a number or a plain CSS value", "timeline.texts[4] place is top, center or bottom"])
    expect(palette([adds]).texts.map((t) => t.kind)).toEqual([...TEXTS.map((t) => t.kind), "neon"])
    // added to a composition, the words go with their style
    const t = fromSequences([{ i: 0, name: "Sequence", label: "A", from: 0, to: 90, lane: 0 }], 30)
    expect(seqEdits(t, { op: "add", kind: "text", clip: { id: "text-1", kind: "text", name: "Hi", props: { style: JSON.stringify(neon) }, start: 0, length: 60 } })).toEqual([{ op: "text", at: 0, frames: 60, text: "Hi", style: neon }])
  })
})

describe("TL11a: a plugin's tools and snapping targets", () => {
  test("a tool is a preset put on the picked clips; a snap grid is marks every so many seconds; both checked", () => {
    const { adds, errors } = readAdds({
      tools: [{ id: "punch-in", title: "Punch in", set: { props: { scale: 1.2 } } }, { id: "slow", title: "Half speed", set: { speed: 0.5 } }, { id: "bad", title: "Bad", set: { props: { colour: 1 } } }, { id: "loud", title: "Loud", set: { volume: 9 } }],
      snaps: [{ id: "bars", title: "Bars at 120", every: 2 }, { id: "fine", title: "Too fine", every: 0.001 }],
    })
    expect(adds.tools?.map((t) => t.id)).toEqual(["punch-in", "slow"])
    expect(adds.snaps).toEqual([{ id: "bars", title: "Bars at 120", every: 2 }])
    expect(errors).toEqual([expect.stringContaining("timeline.tools[2] set.props.colour: a tool places a clip by"), "timeline.tools[3] set.volume runs from 0 to 2", "timeline.snaps[1] is { id, title, every }: every the seconds between marks, from 0.01 to 600"])
    const all = palette([adds])
    expect([all.tools.length, all.snaps.length]).toEqual([2, 1])
    expect(gridFrames(all.snaps[0], 30, 300)).toEqual([60, 120, 180, 240, 300])
    expect(gridFrames({ id: "x", title: "x", every: 0.01 }, 30, 300)).toEqual([])
    expect(palette().tools).toEqual([])
  })
})
