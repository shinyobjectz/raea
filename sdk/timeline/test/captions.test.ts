import { describe, expect, test } from "bun:test"
import { apply, captionLines, type Clip, empty, heard, type Said, TEXTS, type Timeline, seqEdits } from "../src"

const caption = TEXTS.find((s) => s.kind === "caption")!
const w = (text: string, start: number, end: number): Said => ({ text, start, end })
/** a film's scenes as a main track: a 0-30 (film 0-30), b 30-90 (film 30-90), c 90-120 (film 90-120) */
const film = (clips?: Clip[]): Timeline => ({
  ...empty(30),
  tracks: [
    {
      id: "main",
      kind: "main",
      clips: clips ?? [
        { id: "a", kind: "video", start: 0, length: 30, in: 0, source: 120 },
        { id: "b", kind: "video", start: 30, length: 60, in: 30, source: 120 },
        { id: "c", kind: "video", start: 90, length: 30, in: 90, source: 120 },
      ],
    },
  ],
})

describe("TL7c: captions made from what is said", () => {
  test("a line ends after a sentence, at a pause, or once it is full", () => {
    const said = [w("Hello", 0, 10), w("there.", 10, 20), w("This", 21, 25), w("is", 25, 28), w("raea", 28, 40), w("again", 70, 80)]
    expect(captionLines(said, 30)).toEqual([w("Hello there.", 0, 20), w("This is raea", 21, 40), w("again", 70, 80)])
    // seven words at most, and no more characters than a line holds
    const many = Array.from({ length: 10 }, (_, i) => w(`w${i}`, i * 5, i * 5 + 5))
    expect(captionLines(many, 30).map((l) => l.text)).toEqual(["w0 w1 w2 w3 w4 w5 w6", "w7 w8 w9"])
    expect(captionLines([w("supercalifragilistic", 0, 5), w("expialidocious", 5, 10)], 30, { chars: 20 }).map((l) => l.text)).toEqual(["supercalifragilistic", "expialidocious"])
  })

  test("a line ends by the next one's start, and blank words are left out", () => {
    expect(captionLines([w("One.", 0, 30), w(" ", 30, 31), w("Two", 20, 40)], 30)).toEqual([w("One.", 0, 20), w("Two", 20, 40)])
  })

  test("the words are said where the edited timeline plays them", () => {
    const said = [w("in-a", 5, 10), w("in-b", 40, 50), w("in-c", 95, 100)]
    // as cut: the same frames
    expect(heard(film(), said)).toEqual(said)
    // b taken out and the gap closed: c's word comes 60 frames earlier, b's is gone
    const cut = film([
      { id: "a", kind: "video", start: 0, length: 30, in: 0, source: 120 },
      { id: "c", kind: "video", start: 30, length: 30, in: 90, source: 120 },
    ])
    expect(heard(cut, said)).toEqual([w("in-a", 5, 10), w("in-c", 35, 40)])
    // a twice as fast: its word at half the frame; a duplicated says it twice; a reversed clip says none
    const fast = film([
      { id: "a", kind: "video", start: 0, length: 15, in: 0, source: 120, speed: 2 },
      { id: "a+", kind: "video", start: 15, length: 30, in: 0, source: 120 },
      { id: "b", kind: "video", start: 45, length: 60, in: 30, source: 120, reverse: true },
    ])
    expect(heard(fast, said)).toEqual([w("in-a", 3, 5), w("in-a", 20, 25)])
  })

  test("the captions are text clips in the captions' style on a track of their own, one undo away", () => {
    const t = film()
    const r = apply(t, { op: "captions", lines: [w("Hello there.", 0, 20), w("This is raea", 15, 40), w(" ", 50, 60)], style: caption })
    if (!r.ok) throw new Error(r.why)
    const track = r.timeline.tracks.find((k) => k.id === "captions")!
    expect(track).toMatchObject({ kind: "text", name: "Captions" })
    expect(r.timeline.tracks.indexOf(track)).toBeLessThan(r.timeline.tracks.findIndex((k) => k.id === "main"))
    expect(track.clips.map((c) => [c.id, c.name, c.start, c.length])).toEqual([
      ["text-1", "Hello there.", 0, 20],
      ["text-2", "This is raea", 20, 20],
    ])
    expect(JSON.parse(String(track.clips[0].props?.style))).toEqual(caption)
    expect(track.clips[0].props?.said).toBe(true)
    // undone, the track is gone
    const back = apply(r.timeline, r.inverse)
    expect(back.ok && back.timeline).toEqual(t)
    // made again, a second track beside it, with ids no clip has
    const again = apply(r.timeline, { op: "captions", lines: [w("More", 60, 70)], style: caption })
    expect(again.ok && again.timeline.tracks.find((k) => k.id === "captions-2")?.clips.map((c) => c.id)).toEqual(["text-3"])
    // nothing to say: refused
    expect(apply(t, { op: "captions", lines: [w("  ", 0, 10)], style: caption })).toEqual({ ok: false, why: "there are no words to caption" })
  })

  test("in a composition each line is a sequence of words of its own", () => {
    const t = film()
    expect(seqEdits(t, { op: "captions", lines: [w("Hello there.", 0, 20), w("This is raea", 21, 40)], style: caption })).toEqual([
      { op: "text", at: 0, frames: 20, text: "Hello there.", style: caption },
      { op: "text", at: 21, frames: 19, text: "This is raea", style: caption },
    ])
  })
})
