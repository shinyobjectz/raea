// A timeline read from what is there (docs/timeline.feature TL0c, TL14a): a composition's sequences as the host
// lists them (raea-feed host/sequences.ts), or a film with no composition as its scenes and its sound. And an
// operation on a sequences' timeline as the host's own edits to those elements (docs/editing.feature E3b), until
// every operation is a code edit of its own (edits.ts).
import { type Clip, type Ease, empty, end, find, type Frames, type Key, type Timeline, type Track, type Value } from "./model"
import { apply, type Op } from "./ops"
import { EFFECTS, type EffectPart, type TextStyle, textStyleError } from "./registry"
import { COMBO_KINDS, TRANSITION_KINDS } from "@raea/video/edges"
import { CUTOUT_PROPS, cutoutError, cutoutOf, cutoutProps, KEY_PROPS, MASK_PROPS, type Blend, type ChromaKey, type Mask } from "./cutout"
import { ADJUST_KINDS, ADJUST_PROP_LIST, ADJUST_PROPS, adjustError, adjustOf, type Adjust } from "./adjust"

/** A text clip's style, kept in its props as JSON: read back when it is well formed. */
export function textStyleOf(v: unknown): TextStyle | undefined {
  if (typeof v !== "string") return undefined
  try {
    const t = JSON.parse(v) as unknown
    return textStyleError(t) ? undefined : (t as TextStyle)
  } catch {
    return undefined
  }
}
import { effectPartsError } from "@raea/video/effects"

/** An effect clip's parts, kept in its props as JSON: read back when they are well formed. */
export function effectParts(v: unknown): EffectPart[] | undefined {
  if (typeof v !== "string") return undefined
  try {
    const parts = JSON.parse(v) as unknown
    return effectPartsError(parts) ? undefined : (parts as EffectPart[])
  } catch {
    return undefined
  }
}

/** A <Sequence> as the host lists it: its place among the file's elements, its span in the composition's frames,
 *  the lane it sits on (nested ones below their parent, overlapping ones apart), and why it can't be rewritten. */
export interface SeqRow {
  /** its place among the file's <Sequence> elements, in source order: what an edit names it by */
  i: number
  /** its name prop, or what the host calls it when it has none */
  name: string
  /** what its clip's head says, when it isn't the name */
  label?: string
  /** its first frame in the composition */
  from: number
  /** the frame after its last */
  to: number
  /** the lane it sits on: 0 the top, nested ones below their parent, overlapping ones apart */
  lane: number
  /** why it can't be rewritten (a span from a variable, a loop), when it can't */
  locked?: string
  /** the keys its <Nudge> holds, each [frame of the sequence, value, ease?] (TL5d) */
  keys?: Record<string, [number, number, Ease?][]>
  /** how fast the footage it holds plays (its media's playbackRate), when it holds footage at one rate (TL6a) */
  speed?: number
  /** its footage's speed as a curve, each [place 0 to 1, speed] (its media's speedCurve), over `shows` source frames (TL6b) */
  curve?: [number, number][]
  /** under `curve`: how many source frames it shows */
  shows?: number
  /** how it is placed, as its <Nudge> says without keys: moved, scaled, turned, faded, mirrored, cropped (TL3h) */
  look?: SeqLook
  /** the words it shows, when words are all it shows: a text clip, its words changed by renaming it (TL7c) */
  text?: string
  /** the effect it puts on the picture under it, when an <Effect> is all it holds: an effect clip (TL7b) */
  effect?: { kind: string; amount: number; /** a plugin's: the parts that draw it */ parts?: EffectPart[] }
  /** how it comes in and goes out: its <Transition>'s in and out (TL7a) */
  in?: { kind: string; frames: number }
  /** how it goes out: its <Transition>'s out */
  out?: { kind: string; frames: number }
  /** how it moves throughout: its <Transition>'s combo, a cycle every `frames` (TL7e) */
  combo?: { kind: string; frames: number }
  /** a <Video> or an <Audio> of its own (TL0c): it plays where the sequence it is in does, from `startFrom` of its
   *  file, at its own level */
  media?: "video" | "audio"
  /** its media's first frame used, in frames of its file */
  startFrom?: number
  /** its media's level, 1 as recorded */
  volume?: number
  /** a <Video> whose own sound was taken out (muted): its sound is an <Audio> of its own (TL4c) */
  muted?: boolean
}
/** A sequence's still placement: its <Nudge>'s props (sdk/video nudge.tsx). `crop` is left, top, right, bottom. */
export interface SeqLook {
  /** moved right, in px of the composition */
  x?: number
  /** moved down, in px of the composition */
  y?: number
  /** times its size (1 as it is) */
  scale?: number
  /** turned clockwise, in degrees */
  rotate?: number
  /** from 0 (gone) to 1 (as it is) */
  opacity?: number
  /** flipped left to right */
  mirror?: boolean
  /** cut in from each edge, in % of its size: left, top, right, bottom */
  crop?: [number, number, number, number]
  /** what of it is seen: a shape in shares of the frame (TL7f) */
  mask?: Mask
  /** how it is drawn over what is under it, normal when not said (TL7f) */
  blend?: Blend
  /** a colour taken out of it (TL7f) */
  chroma?: ChromaKey
  /** its own Adjust, each kind not 0 (TL7g) */
  adjust?: Adjust
}
const CROPS = ["cropLeft", "cropTop", "cropRight", "cropBottom"]
/** The host's edit to a sequence: its new span, a split, a delete. */
export type SeqEdit =
  | { op: "move"; i: number; from: number; to: number }
  | { op: "split"; i: number; at: number }
  | { op: "delete"; i: number }
  /** the sequence's keyed properties as they now stand: written as its <Nudge keys>, one a property (TL5d) */
  | { op: "keys"; i: number; keys: Record<string, [number, number, Ease?][]> }
  /** the footage it holds played at `speed`, its length following (TL6a) */
  | { op: "speed"; i: number; speed: number }
  /** the footage it holds played by a speed curve over `source` frames of it, the sequence `frames` long (what the
   *  curve takes); null takes the curve off (TL6b) */
  | { op: "curve"; i: number; curve: [number, number][] | null; source: number; frames: number }
  /** its still placement as it now stands: written as its <Nudge>'s props, a prop at its plain value taken off (TL3h) */
  | { op: "nudge"; i: number; look: SeqLook }
  /** a text sequence's words (TL7c) */
  | { op: "words"; i: number; text: string }
  /** words put on the picture from `at` for `frames`: a new sequence after the composition's last (TL7c) */
  | { op: "text"; at: number; frames: number; text: string; /** how the words are drawn: a text style (registry.ts), raea's title when none */ style?: TextStyle }
  /** an effect put on the picture from `at` for `frames`: a new sequence holding an <Effect>, after the composition's last (TL7b) */
  | { op: "effect"; at: number; frames: number; kind: string; amount: number; /** a plugin's effect: the parts that draw it */ parts?: EffectPart[] }
  /** a recording put on the timeline from `at` for `frames` (TL8b): a new sequence holding an <Audio> of the file */
  | { op: "voice"; at: number; frames: number; file: string }
  /** an effect clip's kind or amount */
  | { op: "fx"; i: number; kind?: string; amount?: number }
  /** how it comes in ("in"), goes out ("out") or moves throughout ("combo", TL7e): its <Transition>'s prop set, or
   *  taken off with 0 frames (TL7a) */
  | { op: "edge"; i: number; side: "in" | "out" | "combo"; kind: string; frames: number }
  /** a <Video>'s or an <Audio>'s own level (TL8a) */
  | { op: "level"; i: number; volume: number }
  /** a <Video>'s own sound taken out (TL4c): the <Video> muted, and an <Audio> of the same file, from the same place
   *  at the same pace and level, in a new sequence from `at` for `frames` after the composition's last */
  | { op: "extract"; i: number; at: number; frames: number }
  /** a file of the project's put on the timeline from `at` for `frames` (TL3j): a new sequence holding a <Video>, an
   *  <Img> or an <Audio> of it. In the code it goes after sequence `after` or before sequence `before` (the main
   *  track's clips either side of it, so it is drawn under what is over them), else after the composition's last */
  | { op: "media"; at: number; frames: number; kind: "video" | "image" | "audio"; file: string; after?: number; before?: number }

/** The transitions a composition's <Transition> draws (sdk/video edges.ts). */
export const CODE_KINDS: string[] = TRANSITION_KINDS
/** A sequence's <Nudge> placement, as still values and as keys (sdk/video nudge.tsx). */
export const KEYED = ["x", "y", "scale", "rotate", "opacity"]
/** What a sequence's <Nudge> can key, as a clip's prop and its <Nudge keys> name (TL5a): the placement, and the
 *  numbers inside its mask, its chroma key and its crop ("mask.x" for maskX). */
export const KEY_NAMES: Record<string, string> = {
  ...Object.fromEntries(KEYED.map((k) => [k, k])),
  ...Object.fromEntries((["x", "y", "w", "h", "rotate", "feather", "round"] as const).map((k) => [MASK_PROPS[k], `mask.${k}`])),
  ...Object.fromEntries((["strength", "shadow"] as const).map((k) => [KEY_PROPS[k], `chroma.${k}`])),
  ...Object.fromEntries(CROPS.map((k) => [k, `crop.${k.slice(4).toLowerCase()}`])),
  ...Object.fromEntries(ADJUST_KINDS.map((k) => [ADJUST_PROPS[k], `adjust.${k}`])),
}
/** The clip props a sequence keys. */
export const KEYABLE = Object.keys(KEY_NAMES)
const PROP_OF: Record<string, string> = Object.fromEntries(Object.entries(KEY_NAMES).map(([p, n]) => [n, p]))

const seqId = (i: number) => `seq-${i}`
// a sequence's placement as its clip's props (the crop a prop a side, as a film's clip has it)
const lookProps = (l: SeqLook): Record<string, Value | boolean> => ({
  ...Object.fromEntries(KEYED.flatMap((k) => (typeof l[k as keyof SeqLook] === "number" ? [[k, l[k as keyof SeqLook] as number]] : []))),
  ...(l.mirror && { mirror: true }),
  ...(l.crop && Object.fromEntries(CROPS.map((k, n) => [k, l.crop![n]]))),
  ...((l.mask || l.blend || l.chroma) && cutoutProps(l)),
  ...(l.adjust && Object.fromEntries(Object.entries(l.adjust).map(([k, v]) => [ADJUST_PROPS[k as keyof Adjust], v]))),
})
// the edits to later elements first (an edit that adds or removes an element moves the places after it)
const later = (a: SeqEdit, b: SeqEdit) => ("i" in b ? b.i : 0) - ("i" in a ? a.i : 0)
const seqOf = (id: string): number | undefined => (/^seq-\d+$/.test(id) ? Number(id.slice(4)) : undefined)

/** The composition's sequences as a timeline: a track a lane, the first lane on top as the strip draws it. The first
 *  lane is the main track, its magnet on, only when its clips already run from frame 0 with no gap: a composition's
 *  sequences are where its code puts them, and nothing here moves one that wasn't dragged. */
export function fromSequences(rows: SeqRow[], fps: number): Timeline {
  const media = rows.filter((r) => r.media)
  rows = rows.filter((r) => !r.media)
  const lanes = [...new Set(rows.map((r) => r.lane))].sort((a, b) => a - b)
  const clipsOf = (lane: number): Clip[] => rows.filter((r) => r.lane === lane).sort((a, b) => a.from - b.from).map((r) => ({ id: seqId(r.i), kind: r.effect ? "effect" : r.text === undefined ? "sequence" : "text", name: r.effect ? (EFFECTS.find((e) => e.kind === r.effect!.kind)?.title ?? r.effect.kind) : (r.text ?? r.label ?? r.name), ...(r.effect && { props: { effect: r.effect.kind, amount: r.effect.amount, ...(r.effect.parts && { parts: JSON.stringify(r.effect.parts) }) } }), ...(r.look && Object.keys(r.look).length && { props: lookProps(r.look) }), start: r.from, length: Math.max(1, r.to - r.from), ...(r.locked && { locked: r.locked }), ...(r.speed && r.speed !== 1 && { speed: r.speed }), ...(r.curve?.length && r.shows && { curve: r.curve.map(([at, speed]) => ({ at, speed })), shows: r.shows }), ...(r.keys && Object.keys(r.keys).length && { keys: Object.fromEntries(Object.entries(r.keys).map(([p, ks]) => [PROP_OF[p] ?? p, ks.map(([at, value, ease]): Key => ({ at, value, ...(ease !== undefined && { ease }) }))])) }) }))
  const first = lanes.length ? clipsOf(lanes[0]) : []
  const gapless = first.length > 0 && first.every((c, n) => c.start === (n ? end(first[n - 1]) : 0))
  const tracks: Track[] = lanes.map((lane, n) => ({ id: n === 0 && gapless ? "main" : `overlay-${lane}`, kind: n === 0 && gapless ? "main" : "overlay", clips: clipsOf(lane) }))
  // a transition is on a cut: the part before it goes out and the part after it comes in, the same way
  const by = new Map(rows.map((r) => [seqId(r.i), r]))
  const transitions = tracks.flatMap((k) =>
    k.clips.flatMap((c) => {
      const out = by.get(c.id)?.out
      const next = k.clips.find((o) => o.start === end(c))
      const into = next && by.get(next.id)?.in
      return out && into && out.kind === into.kind ? [{ id: `x-${c.id}`, kind: out.kind, after: c.id, length: out.frames + into.frames }] : []
    }),
  )
  // an edge no cut's transition took is the clip's own In or Out animation (TL7d)
  for (const k of tracks) {
    k.clips = k.clips.map((c) => {
      const r = by.get(c.id), before = k.clips.find((o) => end(o) === c.start)
      const anim = (e?: { kind: string; frames: number }) => (e && e.frames > 0 ? { kind: e.kind, length: Math.min(e.frames, c.length) } : undefined)
      const enter = before && transitions.some((x) => x.after === before.id) ? undefined : anim(r?.in)
      const exit = transitions.some((x) => x.after === c.id) ? undefined : anim(r?.out)
      const combo = r?.combo && r.combo.frames > 0 ? { kind: r.combo.kind, length: r.combo.frames } : undefined
      return { ...c, ...(enter && { enter }), ...(exit && { exit }), ...(combo && { combo }) }
    })
  }
  // footage and sound of their own: a track each kind, more where they play at once; a sound's below the picture
  const mediaTracks = (kind: "video" | "audio"): Track[] => {
    const out: Track[] = []
    for (const r of media.filter((m) => m.media === kind).sort((a, b) => a.from - b.from)) {
      const c: Clip = { id: seqId(r.i), kind, name: r.label ?? r.name, start: r.from, length: Math.max(1, r.to - r.from), in: r.startFrom ?? 0, ...(r.speed && r.speed !== 1 && { speed: r.speed }), ...(r.volume !== undefined && { volume: r.volume }), ...(r.muted && kind === "video" && { muted: true }), ...(r.locked && { locked: r.locked }) }
      const home = out.find((k) => k.clips.every((o) => end(o) <= c.start || o.start >= end(c)))
      if (home) home.clips.push(c)
      else out.push({ id: `${kind === "audio" ? "sound" : "footage"}-${out.length + 1}`, kind: kind === "audio" ? "sound" : "overlay", name: kind === "audio" ? "Sound" : "Footage", clips: [c] })
    }
    return out
  }
  // lanes below the first are nested deeper: drawn under it, as the strip has them
  return { ...empty(fps), tracks: [...tracks, ...mediaTracks("video"), ...mediaTracks("audio")], transitions, magnet: gapless }
}

/** A film with no composition behind it: its scenes as clips of one main track (cut where the picture cuts). Each
 *  clip is the film's picture and its sound together, as footage is until its sound is extracted (TL4c), so a cut,
 *  a move or a speed takes both. Its magnet is on: a scene removed closes up, as a cut of footage does. */
export function fromRender(o: { frames: Frames; fps: number; cuts?: Frames[]; /** the words on screen, each with its span in frames: clips on a text track over the picture (TL7c) */ captions?: { text: string; start: Frames; end: Frames }[] }): Timeline {
  const edges = [0, ...[...new Set(o.cuts ?? [])].filter((c) => c > 0 && c < o.frames).sort((a, b) => a - b), o.frames]
  const scenes: Clip[] = edges.slice(0, -1).map((from, n) => ({ id: `scene-${n + 1}`, kind: "video", name: `Scene ${n + 1}`, start: from, length: edges[n + 1] - from, in: from, source: o.frames }))
  const main: Track = { id: "main", kind: "main", clips: scenes }
  const words = (o.captions ?? [])
    .map((c) => ({ text: c.text, start: Math.max(0, Math.round(c.start)), end: Math.min(o.frames, Math.round(c.end)) }))
    .filter((c) => c.text.trim() && c.end > c.start)
    .sort((a, b) => a.start - b.start)
  if (!words.length) return { ...empty(o.fps), tracks: [main] }
  // captions that overlap the one before start where it ends: a text track holds one line at a time
  const captions: Clip[] = []
  for (const [n, c] of words.entries()) {
    const from = Math.max(c.start, captions.length ? end(captions[captions.length - 1]) : 0)
    if (c.end > from) captions.push({ id: `caption-${n + 1}`, kind: "caption", name: c.text, start: from, length: c.end - from })
  }
  return { ...empty(o.fps), tracks: [{ id: "captions", kind: "text", name: "Captions", clips: captions }, main] }
}

/** A render's cut, as the frames of the source played in order (the hand edit's `segments`): what a film's main
 *  track is after its scenes were moved, trimmed, split and removed. */
export function segmentsOf(t: Timeline): [Frames, Frames][] {
  const main = t.tracks.find((k) => k.kind === "main")
  return (main?.clips ?? []).filter((c) => !c.off && c.in !== undefined).map((c) => [c.in!, c.in! + c.length] as [Frames, Frames])
}

/** What the host is asked to do to the composition's sequences for `op` on timeline `t` (read from them), in order;
 *  or why this operation isn't one the sequences can take yet. Each edit names sequences as they are before it. */
export function seqEdits(t: Timeline, op: Op): SeqEdit[] | { why: string } {
  const row = (id: string) => {
    const i = seqOf(id), at = find(t, id)
    return i !== undefined && at ? { i, clip: at.clip } : undefined
  }
  // a <Video> or an <Audio> plays where its sequence does: its level is its own, everything else its sequence's
  const isMedia = (id: string) => ["video", "audio"].includes(find(t, id)?.clip.kind ?? "") && /^(footage|sound)-/.test(find(t, id)?.track.id ?? "")
  const ids = "ids" in op && op.ids ? op.ids : "id" in op && typeof op.id === "string" ? [op.id] : []
  // footage's sound taken out: its <Video> muted and an <Audio> of the file beside it
  if (op.op === "extract") {
    const at = find(t, op.id)
    if (!at || !isMedia(op.id) || at.clip.kind !== "video") return { why: at?.clip.kind === "sequence" ? "a sequence's sound is the footage in it: take the sound out of its <Video> on the footage track" : "only a <Video>'s sound is taken out of a composition here" }
    const i = seqOf(op.id)
    if (i === undefined) return { why: "that isn't one of the composition's elements" }
    const tried = apply(t, op)
    if (!tried.ok) return { why: tried.why }
    return [{ op: "extract", i, at: at.clip.start, frames: at.clip.length }]
  }
  if (ids.some(isMedia)) {
    if (op.op === "set" && Object.keys(op.set).filter((k) => op.set[k as keyof typeof op.set] !== undefined).join() === "volume") return ids.flatMap((id) => { const i = seqOf(id); return i === undefined ? [] : [{ op: "level", i, volume: Math.round(Math.max(0, op.set.volume!) * 1000) / 1000 } as SeqEdit] })
    return { why: "footage and sound play where their sequence does: move, trim or speed the sequence; here only the level is its own" }
  }
  const each = (ids: string[], f: (r: { i: number; clip: Clip }) => SeqEdit): SeqEdit[] | { why: string } => {
    const rows = ids.map(row)
    return rows.every((r) => r) ? rows.map((r) => f(r!)) : { why: "that isn't one of the composition's sequences" }
  }
  switch (op.op) {
    case "move":
      if (op.track) return { why: "a sequence's lane is its place in the code: it moves in time only" }
      return each(op.ids, (r) => ({ op: "move", i: r.i, from: r.clip.start + Math.round(op.by), to: end(r.clip) + Math.round(op.by) }))
    case "trim":
      return each([op.id], (r) => (op.edge === "start" ? { op: "move", i: r.i, from: Math.min(end(r.clip) - 1, Math.max(0, Math.round(op.to))), to: end(r.clip) } : { op: "move", i: r.i, from: r.clip.start, to: Math.max(r.clip.start + 1, Math.round(op.to)) }))
    case "split": {
      const at = Math.round(op.at)
      const ids = op.ids ?? t.tracks.flatMap((k) => k.clips.filter((c) => c.start < at && at < end(c) && !c.locked).map((c) => c.id))
      // a split makes a sequence after the one it cuts, so later ones are named first
      const r = each(ids, (x) => ({ op: "split", i: x.i, at }))
      return Array.isArray(r) ? r.sort(later) : r
    }
    case "remove": {
      const r = each(op.ids, (x) => ({ op: "delete", i: x.i }))
      return Array.isArray(r) ? r.sort(later) : r
    }
    case "transition":
    case "untransition": {
      const x = op.op === "transition" ? op.transition : t.transitions.find((o) => o.id === op.id)
      if (!x) return { why: "there is no such transition" }
      if (op.op === "transition" && !CODE_KINDS.includes(x.kind)) return { why: `a composition draws ${CODE_KINDS.join(", ")}: "${x.kind}" isn't one its <Transition> has` }
      const a = row(x.after), at = find(t, x.after)
      const b = at ? at.track.clips.find((c) => c.start === end(at.clip)) : undefined
      const nb = b && row(b.id)
      if (!a || !nb) return { why: "a transition sits on a cut between two of the composition's sequences" }
      // half on each side of the cut, no longer than either part
      const len = op.op === "transition" ? Math.max(1, Math.min(Math.round(x.length), a.clip.length * 2, nb.clip.length * 2)) : 0
      return [
        { op: "edge", i: a.i, side: "out", kind: x.kind, frames: Math.ceil(len / 2) },
        { op: "edge", i: nb.i, side: "in", kind: x.kind, frames: Math.floor(len / 2) },
      ]
    }
    case "set": {
      // a speed or a speed curve is the footage's rate; anything else a sequence has is its own code's
      const only = Object.keys(op.set).filter((k) => op.set[k as keyof typeof op.set] !== undefined)
      // an effect clip's kind and amount are its <Effect>'s
      if (only.length === 1 && only[0] === "props" && find(t, op.id)?.clip.kind === "effect") {
        const { effect, amount, parts: _parts, ...other } = op.set.props!
        if (Object.keys(other).length) return { why: `an effect has a kind and an amount, not ${Object.keys(other).join(", ")}` }
        if (effect !== undefined && !EFFECTS.some((e) => e.kind === effect)) return { why: `an effect clip changes to one of raea's own here (a plugin's is added from the bar): raea draws ${EFFECTS.map((e) => e.kind).join(", ")}: "${String(effect)}" isn't one of them` }
        if (amount !== undefined && !(typeof amount === "number" && amount >= 0 && amount <= 1)) return { why: "an effect's amount runs from 0 to 1" }
        return each([op.id], (r) => ({ op: "fx", i: r.i, ...(effect !== undefined && { kind: String(effect) }), ...(amount !== undefined && { amount: Math.round((amount as number) * 1000) / 1000 }) }))
      }
      // how a sequence is placed (moved, scaled, turned, faded, mirrored, cropped, masked, blended, keyed, adjusted): its <Nudge>'s still props
      if (only.length === 1 && only[0] === "props" && find(t, op.id)?.clip.kind !== "effect") {
        const bad = Object.keys(op.set.props!).filter((k) => !KEYED.includes(k) && k !== "mirror" && !CROPS.includes(k) && !CUTOUT_PROPS.includes(k) && !ADJUST_PROP_LIST.includes(k))
        if (bad.length) return { why: `a sequence is placed by its ${[...KEYED, "mirror", "crop", "mask", "blend", "chroma", "adjust"].join(", ")}: ${bad.join(", ")} ${bad.length > 1 ? "are" : "is"} its own code's` }
        const after = apply(t, op)
        if (!after.ok) return { why: after.why }
        const p = find(after.timeline, op.id)?.clip.props ?? {}
        const num = (k: string) => (typeof p[k] === "number" && Number.isFinite(p[k]) ? Math.round((p[k] as number) * 1000) / 1000 : undefined)
        if ((num("scale") ?? 1) <= 0) return { why: "a scale is above 0" }
        const cut = cutoutError(p) ?? adjustError(p)
        if (cut) return { why: cut }
        const crop = CROPS.map((k) => Math.min(0.9, Math.max(0, num(k) ?? 0))) as [number, number, number, number]
        const adjust = adjustOf(p)
        const look: SeqLook = { ...Object.fromEntries(KEYED.flatMap((k) => (num(k) === undefined ? [] : [[k, num(k)]]))), ...(p.mirror === true && { mirror: true }), ...(crop.some((v) => v > 0) && { crop }), ...cutoutOf(p), ...(Object.keys(adjust).length && { adjust }) }
        return each([op.id], (r) => ({ op: "nudge", i: r.i, look }))
      }
      // its In and Out animations (TL7d) and its Combo (TL7e): its <Transition>'s edges, as a cut's transition writes them
      if (only.length && only.every((k) => k === "enter" || k === "exit" || k === "combo")) {
        const after = apply(t, op)
        if (!after.ok) return { why: after.why }
        const now = find(after.timeline, op.id)?.clip
        const out: SeqEdit[] = []
        for (const [key, side] of [["enter", "in"], ["exit", "out"], ["combo", "combo"]] as const) {
          if (!only.includes(key)) continue
          const a = now?.[key], kinds = key === "combo" ? COMBO_KINDS : CODE_KINDS
          if (a && !kinds.includes(a.kind)) return { why: `a composition draws ${kinds.join(", ")}: "${a.kind}" isn't one its <Transition> has` }
          const r = row(op.id)
          if (!r) return { why: "only a composition's sequence comes in or goes out here" }
          out.push({ op: "edge", i: r.i, side, kind: a?.kind ?? (key === "combo" ? COMBO_KINDS[0] : "fade"), frames: a?.length ?? 0 })
        }
        return out
      }
      // a text clip's name is its words
      if (only.length === 1 && only[0] === "name") {
        if (find(t, op.id)?.clip.kind !== "text") return { why: "a sequence's name is its own code's: only a text clip's words change here" }
        if (!op.set.name!.trim()) return { why: "a text clip says something: delete it to take its words off" }
        return each([op.id], (r) => ({ op: "words", i: r.i, text: op.set.name!.trim() }))
      }
      if (only.length === 1 && only[0] === "curve") {
        // the clip as the curve leaves it: the source it shows and how long that now takes
        const after = apply(t, op)
        if (!after.ok) return { why: after.why }
        const c = find(after.timeline, op.id)?.clip
        if (!c) return { why: "that isn't one of the composition's sequences" }
        const two = (v: number) => Math.round(v * 1000) / 1000
        return each([op.id], (r) => ({ op: "curve", i: r.i, curve: c.curve ? c.curve.map((p): [number, number] => [two(p.at), two(p.speed)]) : null, source: Math.round(c.shows ?? c.length), frames: c.length }))
      }
      if (only.length !== 1 || only[0] !== "speed" || !(op.set.speed! > 0)) return { why: `the composition's sequences take a speed or a speed curve, not ${only.join(", ") || "that"}, yet` }
      return each([op.id], (r) => ({ op: "speed", i: r.i, speed: Math.round(op.set.speed! * 1000) / 1000 }))
    }
    case "key":
    case "unkey":
    case "moveKey":
    case "ease": {
      if (!KEYABLE.includes(op.prop)) return { why: `a sequence keys its ${KEYED.join(", ")} and the numbers of its mask, key and crop: ${op.prop} is its own code's` }
      if (op.op === "key" && typeof op.value !== "number") return { why: `${op.prop} is a number` }
      const r = row(op.id)
      if (!r) return { why: "that isn't one of the composition's sequences" }
      // the keys as the operation leaves them, every property: the wrapper's one prop is written whole
      const after = apply(t, op)
      if (!after.ok) return { why: after.why }
      const keys = find(after.timeline, op.id)?.clip.keys ?? {}
      return [{ op: "keys", i: r.i, keys: Object.fromEntries(Object.entries(keys).filter(([p]) => KEYABLE.includes(p)).map(([p, ks]) => [KEY_NAMES[p], ks.map((k): [number, number, Ease?] => (k.ease === undefined || k.ease === "linear" ? [k.at, k.value as number] : [k.at, k.value as number, k.ease]))])) }]
    }
    case "add": {
      // an effect put on the picture: a new sequence of its own, holding an <Effect>
      if (op.clip.kind === "effect") {
        const kind = String(op.clip.props?.effect ?? "")
        // a plugin's effect carries the parts that draw it (as its clip does, in JSON)
        const parts = effectParts(op.clip.props?.parts)
        if (!parts && !EFFECTS.some((e) => e.kind === kind)) return { why: `raea draws ${EFFECTS.map((e) => e.kind).join(", ")}: "${kind}" isn't one of them` }
        if (!/^[a-z][a-z0-9-]*$/.test(kind)) return { why: "an effect's kind is a name in lowercase letters, digits and dashes" }
        const amount = typeof op.clip.props?.amount === "number" ? Math.min(1, Math.max(0, op.clip.props.amount)) : 1
        return [{ op: "effect", at: Math.max(0, Math.round(op.clip.start)), frames: Math.max(1, Math.round(op.clip.length)), kind, amount, ...(parts && { parts }) }]
      }
      // a file of the project's (TL3j): a new sequence of its own holding it. On the main track with the magnet on,
      // the clips after it move on first, and it goes in the code beside the clip before it (or the one after, at
      // the start), so what is over them stays over it
      const file = op.clip.props?.asset
      if (typeof file === "string" && file) {
        const kind = op.clip.kind
        if (kind !== "video" && kind !== "image" && kind !== "audio") return { why: "a file goes on the timeline as footage, a still or a sound" }
        const tried = apply(t, op)
        if (!tried.ok) return { why: tried.why }
        const now = find(tried.timeline, op.clip.id)
        if (!now) return { why: "the file found no track to go on" }
        const media: SeqEdit = { op: "media", at: now.clip.start, frames: now.clip.length, kind, file }
        if (now.track.kind !== "main") return [media]
        const moves: SeqEdit[] = []
        for (const c of now.track.clips) {
          if (c.id === op.clip.id) continue
          const was = find(t, c.id)?.clip, i = seqOf(c.id)
          if (!was || i === undefined) return { why: "the main track holds something that isn't one of the composition's sequences" }
          if (was.start !== c.start || was.length !== c.length) moves.push({ op: "move", i, from: c.start, to: end(c) })
        }
        const k = now.track.clips.findIndex((c) => c.id === op.clip.id)
        const before = now.track.clips[k - 1], after = now.track.clips[k + 1]
        return [...moves, { ...media, ...(before ? { after: seqOf(before.id) } : after ? { before: seqOf(after.id) } : {}) }]
      }
      // a recording made here (its file on this Mac): a new sequence of its own, holding an <Audio>
      if (op.clip.kind === "audio" && typeof op.clip.props?.file === "string" && op.clip.props.file) return [{ op: "voice", at: Math.max(0, Math.round(op.clip.start)), frames: Math.max(1, Math.round(op.clip.length)), file: op.clip.props.file }]
      // words put on the picture: a new sequence of its own
      if (op.clip.kind !== "text") return { why: "only words, effects, recordings and the project's files are added to a composition here" }
      const style = textStyleOf(op.clip.props?.style)
      return [{ op: "text", at: Math.max(0, Math.round(op.clip.start)), frames: Math.max(1, Math.round(op.clip.length)), text: (op.clip.name ?? "Text").trim() || "Text", ...(style && { style }) }]
    }
    case "captions": {
      // captions made from what is said (TL7c): each line a sequence of words of its own, in the captions' style
      if (textStyleError(op.style)) return { why: `a caption's style ${textStyleError(op.style)}` }
      const tried = apply(t, op)
      if (!tried.ok) return { why: tried.why }
      const made = tried.timeline.tracks.find((k) => !t.tracks.some((o) => o.id === k.id))?.clips ?? []
      return made.map((c) => ({ op: "text", at: c.start, frames: c.length, text: c.name ?? "", style: op.style }))
    }
    default:
      return { why: `the composition's sequences don't take "${op.op}" yet` }
  }
}

// ---------------------------------------------------------------- a timeline an item declares (TL0d)

/** A timeline as an agent wrote it on a feed item or a controller's field (its own data: a schedule, a set list, a
 *  shot plan), checked and made whole: what is well formed, and why the rest isn't. Frames are rounded to whole
 *  ones; a track with no kind is an overlay; ids must be there and not repeat. */
export function readTimeline(raw: unknown): { timeline?: Timeline; errors: string[] } {
  const errors: string[] = []
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return { errors: ["a timeline is an object: { fps, tracks: [{ id, kind, clips: [{ id, start, length }] }] }"] }
  const r = raw as Record<string, unknown>
  const fps = typeof r.fps === "number" && r.fps > 0 ? r.fps : 30
  if (r.fps !== undefined && fps !== r.fps) errors.push("fps is a number above 0 (30 was taken)")
  if (!Array.isArray(r.tracks)) return { errors: [...errors, "tracks is a list of { id, kind, clips }"] }
  const seen = new Set<string>()
  const whole = (n: unknown) => (typeof n === "number" && Number.isFinite(n) ? Math.round(n) : NaN)
  const tracks: Track[] = []
  r.tracks.forEach((k, n) => {
    const t = (k ?? {}) as Record<string, unknown>
    const id = typeof t.id === "string" && t.id ? t.id : `track-${n + 1}`
    if (tracks.some((o) => o.id === id)) return void errors.push(`tracks[${n}]: the id ${id} is used twice`)
    const clips: Clip[] = []
    for (const [m, c] of (Array.isArray(t.clips) ? t.clips : []).entries()) {
      const x = (c ?? {}) as Record<string, unknown>
      const at = `tracks[${n}].clips[${m}]`
      const start = whole(x.start), len = whole(x.length)
      if (typeof x.id !== "string" || !x.id) errors.push(`${at} has no id`)
      else if (seen.has(x.id)) errors.push(`${at}: the id ${x.id} is used twice`)
      else if (!(start >= 0) || !(len >= 1)) errors.push(`${at} (${x.id}) starts at 0 or later and is one frame or longer`)
      else {
        seen.add(x.id)
        clips.push({ ...(x as unknown as Clip), id: x.id, kind: typeof x.kind === "string" ? x.kind : "clip", start, length: len })
      }
    }
    tracks.push({ id, kind: typeof t.kind === "string" ? t.kind : "overlay", ...(typeof t.name === "string" && { name: t.name }), ...(t.locked === true && { locked: true }), clips: clips.sort((a, b) => a.start - b.start) })
  })
  const markers = (Array.isArray(r.markers) ? r.markers : []).flatMap((m, n) => {
    const x = (m ?? {}) as Record<string, unknown>
    return whole(x.at) >= 0 ? [{ id: typeof x.id === "string" ? x.id : `m${n + 1}`, at: whole(x.at), ...(typeof x.name === "string" && { name: x.name }), ...(typeof x.color === "string" && { color: x.color }) }] : []
  })
  // its data is where the agent put it: no magnet unless it says so
  return { timeline: { ...empty(fps), tracks, markers, magnet: r.magnet === true, snapping: r.snapping !== false, linkage: r.linkage === true }, errors }
}
