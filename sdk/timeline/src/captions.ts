// Captions made from what is said (docs/timeline.feature TL7c, CapCut's auto captions): the transcript's words as
// the edited timeline says them, gathered into the lines a caption track shows one at a time. The `captions`
// operation (ops.ts) puts the lines on the timeline.
import { type Clip, end, type Frames, speedOf, type Timeline } from "./model"

/** A word, or a line of them: its text, its first frame and the frame after it ends. */
export interface Said {
  text: string
  start: Frames
  end: Frames
}

/** How words are gathered into lines. */
export interface LineRules {
  /** the most words on a line (7) */
  words?: number
  /** the most characters on a line, spaces counted (42) */
  chars?: number
  /** a pause at least this long starts a new line (0.6 s) */
  gap?: Frames
}

/** A clip that plays the film's own words as they were said: footage at a steady speed, forward. */
const speaks = (c: Clip) => !c.off && c.in !== undefined && !c.curve && !c.freeze && !c.reverse

/** The words as the edited timeline says them. `words` are in the film's own frames (where its transcript has them);
 *  each is said where a main-track clip plays that part of the film, at that clip's speed. A word in a scene taken out
 *  is gone, a scene duplicated says its words twice, and a clip curved, reversed or frozen says none. */
export function heard(t: Timeline, words: Said[]): Said[] {
  const main = (t.tracks.find((k) => k.kind === "main" && !k.hidden)?.clips ?? []).filter(speaks)
  const out: Said[] = []
  for (const w of words) {
    for (const c of main) {
      const v = speedOf(c), from = c.in!
      if (w.start < from || w.start >= from + c.length * v) continue
      const start = Math.round(c.start + (w.start - from) / v)
      const stop = Math.min(end(c), Math.round(c.start + (w.end - from) / v))
      out.push({ text: w.text, start, end: Math.max(start + 1, stop) })
    }
  }
  return out.sort((a, b) => a.start - b.start)
}

/** The words gathered into caption lines: a line ends after a sentence does, at a pause, or once it holds as many
 *  words or characters as it may. Each line runs from its first word to its last and ends by the next line's start. */
export function captionLines(words: Said[], fps: number, o: LineRules = {}): Said[] {
  const most = o.words ?? 7, chars = o.chars ?? 42, gap = o.gap ?? Math.round(fps * 0.6)
  const lines: Said[] = []
  let line: Said[] = []
  const close = () => {
    if (line.length) lines.push({ text: line.map((w) => w.text).join(" "), start: line[0].start, end: line[line.length - 1].end })
    line = []
  }
  const said = words.map((w) => ({ ...w, text: w.text.trim() })).filter((w) => w.text).sort((a, b) => a.start - b.start)
  for (const w of said) {
    const last = line[line.length - 1]
    const long = [...line, w].map((x) => x.text).join(" ").length > chars
    if (last && (/[.!?…]["'”’)\]]?$/.test(last.text) || w.start - last.end >= gap || line.length >= most || long)) close()
    line.push(w)
  }
  close()
  return lines.map((l, i) => ({ ...l, end: Math.max(l.start + 1, Math.min(l.end, lines[i + 1]?.start ?? l.end)) }))
}
