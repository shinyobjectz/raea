// A clip's mask, blend and chroma key (docs/timeline.feature TL7f): its props, the preview's look, and a
// composition's <Nudge mask blend chroma>.
import { describe, expect, test } from "bun:test"
import { CUTOUT_PROPS, cutoutError, cutoutLook, cutoutOf, cutoutProps, fromSequences, maskPlain, seqEdits } from "../src"

describe("TL7f: a clip's cutout is its props", () => {
  test("read from props: a mask when its kind is one, a blend other than normal, a key when it has a colour", () => {
    expect(cutoutOf({ mask: "circle", maskX: 0.3, maskFeather: 0.2, maskInvert: true, blend: "screen", keyColor: "#00ff00", keyStrength: 0.7 })).toEqual({
      mask: { kind: "circle", x: 0.3, feather: 0.2, invert: true },
      blend: "screen",
      chroma: { color: "#00ff00", strength: 0.7 },
    })
    // taken off by their plain values
    expect(cutoutOf({ mask: "none", maskX: 0.3, blend: "normal", keyColor: "" })).toEqual({})
    expect(cutoutOf(undefined)).toEqual({})
  })

  test("written as every prop, what it hasn't got at its plain value, so a set takes off the rest", () => {
    expect(cutoutProps({ mask: { kind: "rect", w: 0.4, h: 0.3, round: 0.5 } })).toEqual({ mask: "rect", maskW: 0.4, maskH: 0.3, maskRound: 0.5, maskInvert: false, blend: "normal", keyColor: "" })
    expect(cutoutProps({})).toEqual({ mask: "none", maskInvert: false, blend: "normal", keyColor: "" })
    const c = { mask: { kind: "star" as const, x: 0.25, rotate: 15, invert: true }, blend: "multiply" as const, chroma: { color: "#1040e0", shadow: 0.4 } }
    expect(cutoutOf(cutoutProps(c))).toEqual(c)
    for (const k of Object.keys(cutoutProps(c))) expect(CUTOUT_PROPS).toContain(k)
  })

  test("what is wrong is said: a mask or blend it hasn't got, a value out of range, a grey key", () => {
    expect(cutoutError({ mask: "moon" })).toMatch(/a mask is one of line, mirror, circle, rect, heart, star, or none/)
    expect(cutoutError({ blend: "glow" })).toMatch(/a blend is one of normal, darken, screen/)
    expect(cutoutError({ mask: "circle", maskFeather: 3 })).toMatch(/feather is from 0 to 1/)
    expect(cutoutError({ keyColor: "#777" })).toMatch(/is grey/)
    expect(cutoutError({ mask: "heart", blend: "overlay", keyColor: "#0f0" })).toBeUndefined()
  })

  test("the preview draws it as <Nudge> does: the mask image, the blend, and the key's filter to put beside the picture", () => {
    const look = cutoutLook({ mask: "circle", maskInvert: true, blend: "screen", keyColor: "#00ff00" }, 1920, 1080)
    expect(look.style.maskImage).toMatch(/^linear-gradient\(#000, #000\), url\("data:image\/svg\+xml,/)
    expect(look.style).toMatchObject({ mixBlendMode: "screen", filter: "url(#raea-key-00ff00-50-20)", maskComposite: "exclude" })
    expect(look.key?.id).toBe("raea-key-00ff00-50-20")
    expect(look.key?.matrix.split(" ")).toHaveLength(20)
    // a wrong one draws nothing rather than throwing in the editor
    expect(cutoutLook({ mask: "moon" }, 1920, 1080)).toEqual({ style: {} })
    expect(cutoutLook({}, 1920, 1080)).toEqual({ style: {} })
  })

  test("a mask chosen starts round in the middle of the frame", () => {
    expect(maskPlain("circle", 1080, 1920)).toMatchObject({ x: 0.5, y: 0.5, w: 0.5, h: 0.28125 })
  })
})

describe("TL7f: in a composition it is the sequence's <Nudge mask blend chroma>", () => {
  const row = { i: 0, name: "Sequence", label: "First scene", from: 0, to: 90, lane: 0 }

  test("read back as the clip's props, beside its placement", () => {
    const t = fromSequences([{ ...row, look: { scale: 1.2, mask: { kind: "heart", feather: 0.3 }, blend: "screen", chroma: { color: "#00ff00" } } }], 30)
    expect(t.tracks[0].clips[0].props).toEqual({ scale: 1.2, mask: "heart", maskFeather: 0.3, maskInvert: false, blend: "screen", keyColor: "#00ff00" })
    // a sequence with none has none of the props
    expect(fromSequences([{ ...row, look: { scale: 1.2 } }], 30).tracks[0].clips[0].props).toEqual({ scale: 1.2 })
  })

  test("set on a clip, the whole placement goes to the host with its mask, blend and key; taken off, they go", () => {
    const t = fromSequences([{ ...row, look: { scale: 1.2 } }], 30)
    const masked = seqEdits(t, { op: "set", id: "seq-0", set: { props: { mask: "circle", maskW: 0.3, maskH: 0.5, blend: "multiply", keyColor: "#0f0", keyStrength: 0.6 } } })
    expect(masked).toEqual([{ op: "nudge", i: 0, look: { scale: 1.2, mask: { kind: "circle", w: 0.3, h: 0.5 }, blend: "multiply", chroma: { color: "#0f0", strength: 0.6 } } }])
    const had = fromSequences([{ ...row, look: { scale: 1.2, mask: { kind: "circle" }, blend: "screen", chroma: { color: "#0f0" } } }], 30)
    expect(seqEdits(had, { op: "set", id: "seq-0", set: { props: { mask: "none", blend: "normal", keyColor: "" } } })).toEqual([{ op: "nudge", i: 0, look: { scale: 1.2 } }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { props: { keyColor: "#808080" } } })).toEqual({ why: "a chroma key's colour needs a hue to key by: #808080 is grey" })
  })
})
