// The timeline's model (docs/timeline.feature TL0). Time is whole frames (TL0a): a clip's start, length and trim, a
// key and a marker are integers; seconds and timecodes are drawn from them, never the other way.

/** A count of frames, always whole (TL0a). */
export type Frames = number

/** A track's kind, in the order they stack from the top (TL1a): effect, text, overlay, main, sound. A plugin adds
 *  its own (TL11a); an unknown kind sorts with the overlays. */
export type TrackKind = "effect" | "text" | "overlay" | "main" | "sound" | (string & {})
export const ORDER: TrackKind[] = ["effect", "text", "overlay", "main", "sound"]

/** How a value travels from one key to the next: a straight line, no travel (it holds, then jumps), or a cubic
 *  bezier's two handles [x1, y1, x2, y2] as CSS writes them (TL5c). */
export type Ease = "linear" | "hold" | [number, number, number, number]

/** A property's value: a number, a list of numbers (a colour, a point) or a string. */
export type Value = number | number[] | string

/** A property's value at a frame of its clip (frames from the clip's start), and how it leaves for the next key. */
export interface Key {
  /** the frame it is at, counted from its clip's start */
  at: Frames
  /** the property's value there: a number, a list of them (a colour, a point) or a string */
  value: Value
  /** how the value travels to the next key (linear when unset) */
  ease?: Ease
}

/** A point of a speed curve (TL6b): how fast the source plays at a place in it (0 its first frame shown, 1 its last). */
export interface SpeedPoint {
  /** where in the source, from 0 (its first frame shown) to 1 (its last) */
  at: number
  /** how fast it plays there: 1 as recorded, 2 twice as fast, 0.5 half */
  speed: number
}

/** A mark at a frame (TL2e): a beat, a comment's time (TL12b) or one put there by hand. */
export interface Marker {
  /** its own id, unique on the timeline */
  id: string
  /** its frame on the timeline (on a clip: from the clip's start) */
  at: Frames
  /** what it says, shown on hover */
  name?: string
  /** its colour, any CSS colour */
  color?: string
}

/** How a clip comes in or goes out on its own (TL7d): a transition's kind over its own first or last frames. */
export interface Anim {
  /** fade, slide, wipe, zoom or a plugin's (TRANSITIONS) */
  kind: string
  /** how many of the clip's frames it takes */
  length: Frames
}

/** A span of something on a track: footage, a sound, words, a composition's sequence, an effect. */
export interface Clip {
  /** its own id, unique on the timeline */
  id: string
  /** what it is: "video", "audio", "text", "sequence", "component", "effect", a plugin's own */
  kind: string
  /** what its head says (its file's or its sequence's name when unset) */
  name?: string
  /** where it starts on the timeline */
  start: Frames
  /** how long it lasts on the timeline */
  length: Frames
  /** media: the first source frame it shows */
  in?: Frames
  /** media: how many frames the source has (unset: no end to trim against) */
  source?: Frames
  /** a constant speed (1 when unset, TL6a) */
  speed?: number
  /** a speed curve, which wins over `speed` (TL6b) */
  curve?: SpeedPoint[]
  /** under a curve: how many source frames the clip shows (its length is the time the curve takes over them) */
  shows?: Frames
  /** footage played backward: its last source frame first (TL3h) */
  reverse?: boolean
  /** a still: the source frame `in` held for the clip's whole length (TL3h) */
  freeze?: boolean
  /** turned off: drawn dimmed, left out of the picture and the sound (TL3g) */
  off?: boolean
  /** footage whose own sound was taken out onto a sound track (TL4c): its picture plays silent */
  muted?: boolean
  /** it can't be changed here, and why ("set in code", TL0c) */
  locked?: string
  /** clips with one group pick and move as one (TL3g) */
  group?: string
  /** its still values by property (x, y, scale, rotate, opacity, text, a plugin's): a keyed one is ignored */
  props?: Record<string, Value | boolean>
  /** its keys by property, in frame order (TL5) */
  keys?: Record<string, Key[]>
  /** sound: its level, 1 as recorded (TL8a) */
  volume?: number
  /** sound: how long it fades in from its start */
  fadeIn?: Frames
  /** sound: how long it fades out to its end */
  fadeOut?: Frames
  /** the picture or words: how it comes in over its first frames (TL7d, CapCut's In animation) */
  enter?: Anim
  /** the picture or words: how it goes out over its last frames (TL7d, CapCut's Out animation) */
  exit?: Anim
  /** the picture or words: how it moves over its whole length (TL7e, CapCut's Combo and its text Loop), a cycle every
   *  `length` frames from its first; its own length plays it once */
  combo?: Anim
  /** its element in the code (file:line:col, docs/editing.feature E2) */
  at?: string
  /** markers on the clip itself, in frames from its start (TL2e) */
  markers?: Marker[]
  /** a compound clip (TL3g): the clips it holds, each with the kind of track it came from, their starts in frames
   *  from this clip's own start */
  inside?: { kind: TrackKind; clip: Clip }[]
}

/** A row of clips that don't overlap, with its head's switches (TL1b). */
export interface Track {
  /** its own id, unique on the timeline */
  id: string
  /** what it holds, which places it in the stack (TL1a) */
  kind: TrackKind
  /** what its head says (its kind's title when unset) */
  name?: string
  /** its clips can't be picked or changed */
  locked?: boolean
  /** left out of the picture */
  hidden?: boolean
  /** left out of the sound */
  muted?: boolean
  /** drawn this tall, in px (TL1c) */
  height?: number
  /** its clips, in start order */
  clips: Clip[]
}

/** A transition on the cut between two clips that touch on one track (TL7a): the cut is the end of clip `after`,
 *  so it follows that cut wherever an edit moves it. */
export interface Transition {
  /** its own id, unique on the timeline */
  id: string
  /** its kind: fade, slide, wipe, zoom or a plugin's (TRANSITIONS) */
  kind: string
  /** the clip whose end is the cut it sits on */
  after: string
  /** how long it takes, centred on the cut */
  length: Frames
}

/** A whole timeline: its tracks top to bottom, its marks, its transitions and its three switches. */
export interface Timeline {
  /** frames a second, which every Frames value counts in */
  fps: number
  /** its tracks, top to bottom */
  tracks: Track[]
  /** marks on the ruler (TL2e) */
  markers: Marker[]
  /** transitions on cuts (TL7a) */
  transitions: Transition[]
  /** the main track holds no gap (P, TL4a) */
  magnet: boolean
  /** edges snap to cuts, the playhead, marks and beats (N, TL4b) */
  snapping: boolean
  /** what sits over a main clip moves with it (~, TL4c) */
  linkage: boolean
}

export const empty = (fps = 30): Timeline => ({ fps, tracks: [], markers: [], transitions: [], magnet: true, snapping: true, linkage: true })

export const end = (c: Clip): Frames => c.start + c.length
/** The timeline's length: its last clip's end. */
export const length = (t: Timeline): Frames => Math.max(0, ...t.tracks.flatMap((k) => k.clips.map(end)))
export const clips = (t: Timeline): Clip[] => t.tracks.flatMap((k) => k.clips)
export const find = (t: Timeline, id: string): { track: Track; clip: Clip } | undefined => {
  for (const track of t.tracks) {
    const clip = track.clips.find((c) => c.id === id)
    if (clip) return { track, clip }
  }
}
/** A clip's speed where it is constant. */
export const speedOf = (c: Clip): number => (c.speed && c.speed > 0 ? c.speed : 1)

/** mm:ss:ff, drawn from frames (TL0a). */
export function timecode(frames: Frames, fps: number): string {
  const f = Math.max(0, Math.round(frames))
  const whole = Math.round(fps)
  const two = (n: number) => String(n).padStart(2, "0")
  return `${two(Math.floor(f / whole / 60))}:${two(Math.floor(f / whole) % 60)}:${two(f % whole)}`
}

/** Kinds a plugin said stack under the picture, with the sound (registry.ts sets them). */
export const under = new Set<string>()

/** Where a track of this kind goes among the others (TL1a): after the last of its own kind, else in ORDER's place. */
export function slot(tracks: Track[], kind: TrackKind): number {
  const rank = (k: TrackKind) => (ORDER.includes(k) ? ORDER.indexOf(k) : ORDER.indexOf("overlay"))
  // sound tracks grow downward (a new one goes under the others); every other kind grows upward
  if (kind === "sound" || under.has(kind)) return tracks.length
  const first = tracks.findIndex((t) => rank(t.kind) >= rank(kind))
  return first < 0 ? tracks.length : first
}
