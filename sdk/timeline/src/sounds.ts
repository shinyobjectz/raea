// A sound's own tracks as a timeline (docs/timeline.feature TL14b): the tracks a sound in the feed lists, read in,
// and the file clips a hand edit moved, trimmed or turned up, read back out for the host to write.
import { type Clip, empty, end, find, type Timeline } from "./model"

/** A track of a sound in the feed as its host lists it (raea-feed host/sounds.ts), in seconds of the render. */
export interface SoundTrack {
  /** its id in the sound's file */
  id: string
  /** what its head says */
  name: string
  /** what it holds: notes (a part played from notes), clips (files placed in time) or lines (words sung or said) */
  kind: "notes" | "clips" | "lines"
  /** the bus it plays through */
  bus: string
  /** its clips, each with its span in seconds, what it is, its file, where in the file it starts and its level in dB */
  clips: { id: string; from: number; to: number; label: string; src?: string; offset?: number; gainDb?: number }[]
}
/** A clip's span and level, in seconds and dB: what the host writes back for a clip that was moved, trimmed or
 *  turned up or down (raea-feed host/clips.ts). */
export interface ClipSpan {
  /** where it starts, in seconds of the render */
  start: number
  /** where it ends, in seconds of the render */
  end: number
  /** where in its file it starts playing, in seconds */
  offset: number
  /** its level change, in dB (0 as recorded) */
  gainDb: number
}

const level = (db: number) => Math.round(10 ** (db / 20) * 1000) / 1000
const dB = (v: number) => Math.round(20 * Math.log10(Math.max(v, 0.001)) * 10) / 10

/** A sound's tracks as a timeline: a sound track each, by its bus; a clip of a file keeps its place in the file
 *  (its offset) and its level. Notes and lines are clips too, locked here: they are edited where they are written
 *  (the roll, the words). No magnet: a mix's clips are where they were put. */
export function fromTracks(tracks: SoundTrack[], fps: number): Timeline {
  const f = (s: number) => Math.round(s * fps)
  return {
    ...empty(fps),
    magnet: false,
    tracks: tracks.map((k) => ({
      id: k.id,
      kind: "sound" as const,
      name: k.name,
      clips: k.clips
        .map((c): Clip => ({ id: c.id, kind: k.kind === "clips" ? "audio" : k.kind, name: c.label, start: f(c.from), length: Math.max(1, f(c.to) - f(c.from)), ...(c.src ? { in: f(c.offset ?? 0), at: c.src, volume: level(c.gainDb ?? 0) } : { locked: k.kind === "notes" ? "edited on its roll" : "edited in its words" }) }))
        .sort((a, b) => a.start - b.start),
    })),
  }
}

/** A clip of a file as the host is told it: where it plays, how far into its file, how loud. */
const spanOf = (c: Clip, fps: number): ClipSpan => ({ start: c.start / fps, end: end(c) / fps, offset: (c.in ?? 0) / fps, gainDb: dB(c.volume ?? 1) })

/** The file clips that differ between the timeline as it was read and as it stands, each with its span before and
 *  after: what a sound's hand edit carries (its `clips`). A clip that was removed or made here isn't one of these. */
export function clipEdits(read: Timeline, now: Timeline): { id: string; src: string; from: ClipSpan; to: ClipSpan }[] {
  const out: { id: string; src: string; from: ClipSpan; to: ClipSpan }[] = []
  for (const k of read.tracks)
    for (const was of k.clips) {
      const is = find(now, was.id)?.clip
      if (!was.at || !is) continue
      const a = spanOf(was, read.fps), b = spanOf(is, now.fps)
      if (a.start !== b.start || a.end !== b.end || a.offset !== b.offset || a.gainDb !== b.gainDb) out.push({ id: was.id, src: was.at, from: a, to: b })
    }
  return out
}
