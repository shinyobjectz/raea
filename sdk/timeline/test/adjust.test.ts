// A clip's own Adjust (docs/timeline.feature TL7g): its props, the preview's look, and a composition's <Nudge adjust>.
import { describe, expect, test } from "bun:test"
import { ADJUST_PROPS, adjustError, adjustLook, adjustOf, adjustProps, fromSequences, KEY_NAMES, seqEdits } from "../src"

describe("TL7g: a clip's Adjust is its props", () => {
  test("one prop a kind, read back as the kinds not 0, written whole so a set puts it all", () => {
    expect(ADJUST_PROPS.brightness).toBe("adjustBrightness")
    expect(adjustOf({ adjustContrast: 0.3, adjustSaturation: 0, adjustGrain: 0.5, scale: 2 })).toEqual({ contrast: 0.3, grain: 0.5 })
    const props = adjustProps({ contrast: 0.3333 })
    expect(Object.keys(props)).toHaveLength(12)
    expect(props.adjustContrast).toBe(0.333)
    expect(props.adjustVignette).toBe(0)
    expect(adjustOf(adjustProps({}))).toEqual({})
  })

  test("what is wrong is said: a value out of its range, or not a number", () => {
    expect(adjustError({ adjustSharpen: -0.2 })).toMatch(/sharpen is from 0 to 1/)
    expect(adjustError({ adjustContrast: "lots" })).toMatch(/contrast is a number/)
    expect(adjustError({ adjustContrast: -1, adjustFade: 1 })).toBeUndefined()
  })

  test("the preview's look: the filter's id and steps, and the vignette's background; nothing for a wrong one", () => {
    const look = adjustLook({ adjustContrast: 0.5, adjustVignette: 0.5 })
    expect(look.filter?.id).toBe("raea-adjust-con50")
    expect(look.filter?.steps[0].tag).toBe("feComponentTransfer")
    expect(look.vignette).toContain("radial-gradient")
    expect(adjustLook({ adjustVignette: 0.5 }).filter).toBeUndefined()
    expect(adjustLook({ adjustContrast: 9 })).toEqual({})
    expect(adjustLook(undefined)).toEqual({})
  })

  test("each kind is keyed as adjust.<kind>", () => {
    expect(KEY_NAMES.adjustTemperature).toBe("adjust.temperature")
  })
})

describe("TL7g: in a composition it is the sequence's <Nudge adjust>", () => {
  const row = { i: 0, name: "Sequence", label: "First scene", from: 0, to: 90, lane: 0 }

  test("read back as the clip's props, beside its placement", () => {
    const t = fromSequences([{ ...row, look: { scale: 1.2, adjust: { contrast: 0.3, vignette: 0.6 } } }], 30)
    expect(t.tracks[0].clips[0].props).toEqual({ scale: 1.2, adjustContrast: 0.3, adjustVignette: 0.6 })
  })

  test("set on a clip, the whole placement goes to the host with its adjustment; all 0, it goes", () => {
    const t = fromSequences([{ ...row, look: { scale: 1.2 } }], 30)
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { props: { adjustExposure: 0.4 } } })).toEqual([{ op: "nudge", i: 0, look: { scale: 1.2, adjust: { exposure: 0.4 } } }])
    const had = fromSequences([{ ...row, look: { scale: 1.2, adjust: { exposure: 0.4 } } }], 30)
    expect(seqEdits(had, { op: "set", id: "seq-0", set: { props: adjustProps({}) } })).toEqual([{ op: "nudge", i: 0, look: { scale: 1.2 } }])
    expect(seqEdits(t, { op: "set", id: "seq-0", set: { props: { adjustGrain: 3 } } })).toEqual({ why: "an adjust's grain is from 0 to 1, got 3" })
  })

  test("a key on an adjustment is its <Nudge keys> by adjust.<kind>, and read back", () => {
    const t = fromSequences([{ ...row }], 30)
    expect(seqEdits(t, { op: "key", id: "seq-0", prop: "adjustSaturation", at: 10, value: -0.5 })).toEqual([{ op: "keys", i: 0, keys: { "adjust.saturation": [[10, -0.5]] } }])
    const read = fromSequences([{ ...row, keys: { "adjust.saturation": [[0, 0], [30, -1]] } }], 30)
    expect(read.tracks[0].clips[0].keys).toEqual({ adjustSaturation: [{ at: 0, value: 0 }, { at: 30, value: -1 }] })
  })
})
