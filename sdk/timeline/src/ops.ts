// Every edit to a timeline as an operation (docs/timeline.feature TL0b): a pure function from a timeline to the
// next, refused with its reason when it can't apply, and answered with its inverse so undo is another operation.
// After each one the timeline is settled: the main track closed while its magnet is on (TL4a), what sits over a
// main clip carried with it while linkage is on (TL4c), transitions kept only on cuts that still exist.
import { cut as cutKeys, sorted } from "./keys"
import { type Anim, type Clip, end, find, type Frames, type Key, type Marker, slot, speedOf, type SpeedPoint, type Timeline, type Track, type TrackKind, type Transition, type Value, type Ease } from "./model"
import { fits } from "./snap"
import { played } from "./speed"
import type { TextStyle } from "./registry"

export type Op =
  /** clips moved in time by `by` frames, and (one kind's clips) onto track `track` */
  | { op: "move"; ids: string[]; by: Frames; track?: string }
  /** a clip's start or end dragged to the timeline frame `to` */
  | { op: "trim"; id: string; edge: "start" | "end"; to: Frames }
  /** the clips `ids` split at `at`; with none named, every clip under it (Cmd+B, Cmd+Shift+B: TL3d) */
  | { op: "split"; at: Frames; ids?: string[] }
  /** clips removed; `ripple` closes the gap on any track (Shift+Delete, TL3e) */
  | { op: "remove"; ids: string[]; ripple?: boolean }
  /** what is left (Q) or right (W) of `at` on a clip removed, the gap closed (TL3e) */
  | { op: "cut"; id: string; side: "left" | "right"; at: Frames }
  | { op: "duplicate"; ids: string[] }
  /** the clips made one compound clip, where the first of them on the main track (else the first) was (TL3g) */
  | { op: "compound"; ids: string[]; id?: string; name?: string }
  /** a compound clip put back as the clips it holds */
  | { op: "uncompound"; id: string }
  /** a clip put on the timeline: on `track`, else the first of `kind` with room, else a new track of that kind */
  | { op: "add"; kind: TrackKind; clip: Clip; track?: string }
  /** footage's own sound taken out onto a sound track (Cmd+Shift+S, TL4c): a sound clip `sound` (an id made from the
   *  footage's when unnamed) over the same span, from the same place in the file at the same pace and level, and the
   *  footage silent from then on, so its sound moves, trims and fades on its own */
  | { op: "extract"; id: string; sound?: string }
  /** a clip's own fields; `props` are merged into its props; a speed or a curve changes its length (TL6) */
  /** a still of the frame under `at` on a clip (the main track's, when none is named), `length` frames long, put in
   *  there: the clip is split and the still goes between its halves (TL3h) */
  | { op: "freeze"; at: Frames; length: Frames; id?: string }
  | { op: "set"; id: string; set: Partial<Pick<Clip, "name" | "off" | "volume" | "fadeIn" | "fadeOut" | "speed" | "group" | "reverse">> & { curve?: SpeedPoint[] | null; props?: Record<string, Value | boolean>; /** its In or Out animation (TL7d); null takes it off */ enter?: Anim | null; exit?: Anim | null; /** its Combo (TL7e), a cycle every `length` frames; null takes it off */ combo?: Anim | null } }
  /** a property's key at a frame of its clip: made, or its value (and ease) changed (TL5a) */
  | { op: "key"; id: string; prop: string; at: Frames; value: Value; ease?: Ease }
  | { op: "unkey"; id: string; prop: string; at: Frames }
  | { op: "moveKey"; id: string; prop: string; at: Frames; to: Frames }
  | { op: "ease"; id: string; prop: string; at: Frames; ease: Ease }
  /** a marker made or changed (by its id), on the timeline or on clip `clip` (TL2e) */
  | { op: "mark"; marker: Marker; clip?: string }
  | { op: "unmark"; id: string; clip?: string }
  | { op: "track"; id: string; set: Partial<Pick<Track, "name" | "locked" | "hidden" | "muted" | "height">> }
  /** a transition put on the cut after clip `after`, or changed (TL7a) */
  | { op: "transition"; transition: Transition }
  | { op: "untransition"; id: string }
  /** captions made from what is said (TL7c): each line a text clip in `style`, on a track of their own named Captions
   *  (captions.ts makes the lines) */
  | { op: "captions"; lines: { text: string; start: Frames; end: Frames }[]; style: TextStyle }
  | { op: "option"; set: Partial<Pick<Timeline, "magnet" | "snapping" | "linkage">> }
  /** the timeline as it was: what every inverse is */
  | { op: "restore"; timeline: Timeline }

export type Applied = { ok: true; timeline: Timeline; inverse: Op } | { ok: false; why: string }

class Refused extends Error {}
const no = (why: string): never => {
  throw new Refused(why)
}
const whole = (n: number) => Math.round(n)

/** The operation applied, or why it can't be. */
export function apply(t: Timeline, op: Op): Applied {
  try {
    const next = op.op === "restore" ? op.timeline : settle(t, run(t, op), op)
    return { ok: true, timeline: next, inverse: { op: "restore", timeline: t } }
  } catch (e) {
    if (e instanceof Refused) return { ok: false, why: e.message }
    throw e
  }
}

/** Operations in order, stopping at the first refused: the timeline they give, and each one's inverse (last first). */
export function applyAll(t: Timeline, ops: Op[]): { timeline: Timeline; inverse: Op[]; refused?: { op: Op; why: string } } {
  const inverse: Op[] = []
  for (const op of ops) {
    const r = apply(t, op)
    if (!r.ok) return { timeline: t, inverse, refused: { op, why: r.why } }
    inverse.unshift(r.inverse)
    t = r.timeline
  }
  return { timeline: t, inverse }
}

// ---------------------------------------------------------------- reading and rewriting

/** The clip and its track, when both can be changed. */
function open(t: Timeline, id: string): { track: Track; clip: Clip } {
  const at = find(t, id) ?? no(`there is no clip ${id}`)
  if (at.track.locked) no(`${at.track.name ?? "that track"} is locked`)
  if (at.clip.locked) no(`${at.clip.name ?? at.clip.id} is ${at.clip.locked}`)
  return at
}
const withTracks = (t: Timeline, f: (k: Track) => Track): Timeline => ({ ...t, tracks: t.tracks.map(f) })
const change = (t: Timeline, id: string, f: (c: Clip) => Clip): Timeline => withTracks(t, (k) => (k.clips.some((c) => c.id === id) ? { ...k, clips: k.clips.map((c) => (c.id === id ? f(c) : c)) } : k))
/** An id no clip has, made from `id`. */
function fresh(t: Timeline, id: string, taken: Set<string> = new Set()): string {
  let n = `${id}+`
  while (find(t, n) || taken.has(n)) n += "+"
  taken.add(n)
  return n
}
/** How many source frames a clip shows. */
const shown = (c: Clip): number => (c.curve ? (c.shows ?? c.length / played(c.curve, 1)) : c.length * speedOf(c))
const isMain = (k: Track) => k.kind === "main"

/** `clip` on the timeline: on `prefer` if it has room there, else the first track of `kind` with room, else a new
 *  track of that kind in its place (TL1a, TL4a: a clip dropped over another goes to a track of its own). */
function place(t: Timeline, kind: TrackKind, clip: Clip, prefer?: string): Timeline {
  const ok = (k: Track) => !k.locked && (isMain(k) ? t.magnet || fits(k, clip) : fits(k, clip))
  const want = prefer ? t.tracks.find((k) => k.id === prefer) : undefined
  if (want?.locked) no(`${want.name ?? "that track"} is locked`)
  const home = (want && ok(want) ? want : undefined) ?? (kind === "main" ? undefined : t.tracks.find((k) => k.kind === kind && ok(k))) ?? (kind === "main" ? t.tracks.find((k) => isMain(k) && ok(k)) : undefined)
  if (home) return withTracks(t, (k) => (k.id === home.id ? { ...k, clips: [...k.clips, clip] } : k))
  // no room on the main track (its magnet off): the clip becomes an overlay
  const made: TrackKind = kind === "main" && t.tracks.some(isMain) ? "overlay" : kind
  let n = 1
  while (t.tracks.some((k) => k.id === `${made}-${n}`)) n++
  const tracks = [...t.tracks]
  tracks.splice(slot(tracks, made), 0, { id: `${made}-${n}`, kind: made, clips: [clip] })
  return { ...t, tracks }
}

// ---------------------------------------------------------------- the operations

function run(t: Timeline, op: Exclude<Op, { op: "restore" }>): Timeline {
  switch (op.op) {
    case "move": {
      const moving = op.ids.map((id) => open(t, id))
      if (!moving.length) no("nothing is picked")
      // no further back than the film's first frame: a drag past it stops there
      const by = Math.max(whole(op.by), -Math.min(...moving.map((m) => m.clip.start)))
      const ids = new Set(op.ids)
      // lifted off their tracks, then each put down where it lands: its own track, the one named, or one with room
      let next = withTracks(t, (k) => ({ ...k, clips: k.clips.filter((c) => !ids.has(c.id)) }))
      for (const m of moving) {
        const to = op.track ? (t.tracks.find((k) => k.id === op.track) ?? no(`there is no track ${op.track}`)) : m.track
        if (to.kind !== m.track.kind && !(isMain(to) || isMain(m.track)) ) no(`a ${m.track.kind} clip doesn't go on a ${to.kind} track`)
        next = place(next, to.kind, { ...m.clip, start: m.clip.start + by }, to.id)
      }
      return next
    }
    case "trim": {
      const { track, clip } = open(t, op.id)
      const to = whole(op.to), speed = speedOf(clip)
      if (op.edge === "start") {
        // back out no further than the source's first frame, in no further than one frame of clip
        const least = clip.in === undefined ? 0 : clip.start - Math.floor(clip.in / speed)
        const before = isMain(track) && t.magnet ? 0 : Math.max(0, ...track.clips.filter((c) => c.id !== clip.id && end(c) <= clip.start).map(end))
        const start = Math.min(end(clip) - 1, Math.max(to, least, before, 0))
        const d = start - clip.start
        if (!d) return t
        return change(t, clip.id, (c) => ({ ...c, start, length: c.length - d, ...(c.shows !== undefined && { shows: (c.shows * (c.length - d)) / c.length }), ...(c.in !== undefined && { in: Math.max(0, c.in + Math.round(d * speed)) }), ...(c.keys && { keys: shiftKeys(c.keys, d) }), ...(c.markers && { markers: c.markers.map((m) => ({ ...m, at: m.at - d })).filter((m) => m.at >= 0) }) }))
      }
      const most = clip.source === undefined || clip.curve ? Infinity : clip.start + Math.floor((clip.source - (clip.in ?? 0)) / speed)
      const after = isMain(track) && t.magnet ? Infinity : Math.min(Infinity, ...track.clips.filter((c) => c.id !== clip.id && c.start >= end(clip)).map((c) => c.start))
      const stop = Math.max(clip.start + 1, Math.min(to, most, after))
      return stop === end(clip) ? t : change(t, clip.id, (c) => ({ ...c, length: stop - c.start, ...(c.shows !== undefined && { shows: (c.shows * (stop - c.start)) / c.length }) }))
    }
    case "split": {
      const at = whole(op.at)
      const under = (c: Clip) => c.start < at && at < end(c)
      const named = op.ids ? op.ids.map((id) => open(t, id).clip) : t.tracks.filter((k) => !k.locked).flatMap((k) => k.clips.filter((c) => under(c) && !c.locked))
      const cutting = named.filter(under)
      if (!cutting.length) no("no clip is under the playhead")
      const ids = new Set(cutting.map((c) => c.id)), taken = new Set<string>()
      return withTracks(t, (k) => ({ ...k, clips: k.clips.flatMap((c) => (ids.has(c.id) ? halves(c, at, fresh(t, c.id, taken)) : [c])) }))
    }
    case "remove": {
      const gone = op.ids.map((id) => open(t, id))
      if (!gone.length) no("nothing is picked")
      const ids = new Set(op.ids)
      return withTracks(t, (k) => {
        if (!k.clips.some((c) => ids.has(c.id))) return k
        const out = k.clips.filter((c) => ids.has(c.id))
        // each later clip moves back by what was removed before it
        const back = (c: Clip) => (op.ripple && !isMain(k) ? out.filter((o) => o.start < c.start).reduce((n, o) => n + o.length, 0) : 0)
        return { ...k, clips: k.clips.filter((c) => !ids.has(c.id)).map((c) => (back(c) ? { ...c, start: c.start - back(c) } : c)) }
      })
    }
    case "cut": {
      const { track, clip } = open(t, op.id)
      const at = whole(op.at)
      if (!(clip.start < at && at < end(clip))) no("the playhead isn't on that clip")
      const [left, right] = halves(clip, at, clip.id)
      const kept = op.side === "left" ? { ...right, start: clip.start } : left
      const lost = clip.length - kept.length
      return withTracks(t, (k) => (k.id !== track.id ? k : { ...k, clips: k.clips.map((c) => (c.id === clip.id ? kept : !isMain(k) && c.start >= end(clip) ? { ...c, start: c.start - lost } : c)) }))
    }
    case "duplicate": {
      const taken = new Set<string>()
      let next = t
      for (const id of op.ids) {
        const { track, clip } = open(t, id)
        const copy = { ...clip, id: fresh(t, id, taken), start: end(clip), group: undefined }
        // on a track with no magnet, what comes after makes room for it
        if (!(isMain(track) && t.magnet)) next = withTracks(next, (k) => (k.id === track.id ? { ...k, clips: k.clips.map((c) => (c.start >= end(clip) ? { ...c, start: c.start + clip.length } : c)) } : k))
        next = place(next, track.kind, copy, track.id)
      }
      return next
    }
    case "compound": {
      const members = op.ids.map((id) => open(t, id))
      if (members.length < 2) no("a compound clip is made of two clips or more")
      const from = Math.min(...members.map((m) => m.clip.start)), to = Math.max(...members.map((m) => end(m.clip)))
      const home = (members.find((m) => isMain(m.track)) ?? members[0]).track
      const ids = new Set(op.ids)
      const id = op.id ?? fresh(t, `compound-${members[0].clip.id}`)
      if (find(t, id)) no(`there is already a clip ${id}`)
      const made: Clip = { id, kind: "compound", name: op.name ?? "Compound clip", start: from, length: to - from, inside: members.map((m) => ({ kind: m.track.kind, clip: { ...m.clip, start: m.clip.start - from } })) }
      return place(withTracks(t, (k) => ({ ...k, clips: k.clips.filter((c) => !ids.has(c.id)) })), home.kind, made, home.id)
    }
    case "uncompound": {
      const { clip } = open(t, op.id)
      if (!clip.inside?.length) no(`${clip.name ?? clip.id} isn't a compound clip`)
      // each clip back where it sat in the compound, on a track of the kind it came from
      let next = withTracks(t, (k) => ({ ...k, clips: k.clips.filter((c) => c.id !== clip.id) }))
      for (const m of clip.inside!) next = place(next, m.kind, { ...m.clip, start: clip.start + m.clip.start })
      return next
    }
    case "freeze": {
      const at = whole(op.at)
      const under = (c: Clip) => c.start <= at && at < end(c)
      const host = op.id ? open(t, op.id) : (() => {
        const k = t.tracks.find((x) => isMain(x) && !x.locked) ?? no("there is no main track to hold a still")
        const c = k.clips.find((x) => under(x) && !x.locked) ?? no("no clip is under the playhead")
        return { track: k, clip: c }
      })()
      const c = host.clip
      if (!under(c)) no("the playhead isn't on that clip")
      if (c.in === undefined || c.freeze) no(`${c.name ?? c.id} has no footage to take a still from`)
      const len = Math.max(1, whole(op.length))
      const into = at - c.start
      const first = c.in ?? 0
      const frame = c.reverse ? first + Math.round((c.length - 1 - into) * speedOf(c)) : first + Math.round(into * speedOf(c))
      const still: Clip = { id: fresh(t, `${c.id}-still`), kind: c.kind, name: "Freeze", start: at, length: len, in: frame, source: c.source, freeze: true }
      // the clip's two halves either side of the still (none before it when the playhead is on its first frame),
      // what follows on the track making room
      const parts = into > 0 ? halves(c, at, fresh(t, c.id)) : [undefined, c] as const
      const after = parts[1] ? { ...parts[1], start: at + len } : undefined
      return withTracks(t, (k) => (k.id !== host.track.id ? k : { ...k, clips: [...k.clips.flatMap((x) => (x.id === c.id ? [parts[0], still, after].filter((y): y is Clip => !!y) : [x.start >= end(c) ? { ...x, start: x.start + len } : x]))] }))
    }
    case "extract": {
      const { track, clip } = open(t, op.id)
      const name = clip.name ?? clip.id
      if (track.kind === "sound" || clip.kind === "audio") no(`${name} is sound already`)
      if (clip.in === undefined || clip.freeze) no(`${name} isn't footage: it has no sound of its own to take out`)
      if (clip.muted) no(`${name}'s sound is on a sound track already`)
      const id = op.sound ?? fresh(t, `${clip.id}-sound`)
      if (find(t, id)) no(`there is already a clip ${id}`)
      // the same stretch of the file, played the same way: speed or curve, backward, level and fades
      const pick = <K extends keyof Clip>(...keys: K[]) => Object.fromEntries(keys.filter((k) => clip[k] !== undefined).map((k) => [k, clip[k]])) as Pick<Clip, K>
      const sound: Clip = { id, kind: "audio", name, start: clip.start, length: clip.length, ...pick("in", "source", "speed", "curve", "shows", "reverse", "volume", "fadeIn", "fadeOut", "off"), props: { of: clip.id } }
      const silent = change(t, clip.id, (c) => ({ ...c, muted: true, volume: undefined, fadeIn: undefined, fadeOut: undefined }))
      return place(silent, "sound", sound)
    }
    case "add": {
      if (find(t, op.clip.id)) no(`there is already a clip ${op.clip.id}`)
      if (!(op.clip.length >= 1) || op.clip.start < 0) no("a clip is at least one frame long and starts in the film")
      return place(t, op.kind, { ...op.clip, start: whole(op.clip.start), length: whole(op.clip.length) }, op.track)
    }
    case "set": {
      const { clip } = open(t, op.id)
      const { props, curve, enter, exit, combo, ...rest } = op.set
      let c: Clip = { ...clip, ...rest, ...(props && { props: { ...clip.props, ...props } }) }
      const source = shown(clip)
      if (curve !== undefined || (rest.speed !== undefined && rest.speed !== clip.speed)) {
        if (rest.speed !== undefined && !(rest.speed > 0)) no("a speed is more than 0")
        const points = curve === undefined ? clip.curve : (curve ?? undefined)
        // the same frames of source, at the new pace
        c = { ...c, curve: points, shows: points ? source : undefined, length: Math.max(1, whole(points ? played(points, source) : source / speedOf(c))) }
      }
      // an In or Out animation (TL7d) or a Combo's cycle (TL7e): a kind, over no more of the clip than it has; null takes it off
      for (const [side, a] of [["enter", enter], ["exit", exit], ["combo", combo]] as const) {
        if (a === undefined) continue
        if (a !== null && (typeof a.kind !== "string" || !a.kind || !(a.length >= 1))) no("an animation is a kind and at least one frame")
        const { [side]: _, ...without } = c
        c = a ? { ...without, [side]: { kind: a.kind, length: Math.min(whole(a.length), c.length) } } : without
      }
      return change(t, clip.id, () => c)
    }
    case "key":
    case "unkey":
    case "moveKey":
    case "ease": {
      const { clip } = open(t, op.id)
      const at = whole(op.at)
      const had = clip.keys?.[op.prop] ?? []
      const here = had.find((k) => k.at === at)
      let keys: Key[]
      if (op.op === "key") {
        if (at < 0 || at > clip.length) no("a key is inside its clip")
        keys = [...had.filter((k) => k.at !== at), { at, value: op.value, ...((op.ease ?? here?.ease) !== undefined && { ease: op.ease ?? here?.ease }) }]
      } else {
        if (!here) no(`${op.prop} has no key there`)
        if (op.op === "unkey") keys = had.filter((k) => k.at !== at)
        else if (op.op === "ease") keys = had.map((k) => (k.at === at ? { ...k, ease: op.ease } : k))
        else {
          const to = whole(op.to)
          if (to < 0 || to > clip.length) no("a key is inside its clip")
          keys = [...had.filter((k) => k.at !== at && k.at !== to), { ...here!, at: to }]
        }
      }
      const all = { ...clip.keys, [op.prop]: sorted(keys) }
      if (!keys.length) delete all[op.prop]
      return change(t, clip.id, (c) => ({ ...c, keys: Object.keys(all).length ? all : undefined }))
    }
    case "mark":
    case "unmark": {
      const id = op.op === "mark" ? op.marker.id : op.id
      const put = (ms: Marker[] = []) => [...ms.filter((m) => m.id !== id), ...(op.op === "mark" ? [{ ...op.marker, at: whole(op.marker.at) }] : [])].sort((a, b) => a.at - b.at)
      if (op.clip) return change(t, open(t, op.clip).clip.id, (c) => ({ ...c, markers: put(c.markers) }))
      return { ...t, markers: put(t.markers) }
    }
    case "track": {
      if (!t.tracks.some((k) => k.id === op.id)) no(`there is no track ${op.id}`)
      return withTracks(t, (k) => (k.id === op.id ? { ...k, ...op.set } : k))
    }
    case "transition": {
      const x = op.transition
      const { track, clip } = open(t, x.after)
      const next = track.clips.find((c) => c.start === end(clip)) ?? no("a transition sits on a cut: no clip starts where that one ends")
      // no longer than either clip can give it
      const length = Math.max(1, Math.min(whole(x.length), clip.length, next.length))
      return { ...t, transitions: [...t.transitions.filter((o) => o.id !== x.id && o.after !== x.after), { ...x, length }] }
    }
    case "untransition":
      return { ...t, transitions: t.transitions.filter((o) => o.id !== op.id) }
    case "captions": {
      // one line at a time, in order: a line starting before the one ahead of it ends starts where that one ends
      const taken = new Set(t.tracks.flatMap((k) => k.clips.map((c) => c.id)))
      let n = 1
      const clips: Clip[] = []
      for (const l of [...op.lines].sort((a, b) => a.start - b.start)) {
        const text = l.text.trim()
        const from = Math.max(0, whole(l.start), clips.length ? end(clips[clips.length - 1]) : 0)
        if (!text || whole(l.end) <= from) continue
        while (taken.has(`text-${n}`)) n++
        taken.add(`text-${n}`)
        clips.push({ id: `text-${n}`, kind: "text", name: text, start: from, length: whole(l.end) - from, props: { text: op.style.kind, style: JSON.stringify(op.style), said: true } })
      }
      if (!clips.length) no("there are no words to caption")
      let k = 1
      while (t.tracks.some((x) => x.id === (k === 1 ? "captions" : `captions-${k}`))) k++
      const tracks = [...t.tracks]
      tracks.splice(slot(tracks, "text"), 0, { id: k === 1 ? "captions" : `captions-${k}`, kind: "text", name: "Captions", clips })
      return { ...t, tracks }
    }
    case "option":
      return { ...t, ...op.set }
  }
}

/** A clip's keys after its start moved in by `d` frames (out, when negative): they stay at their timeline frames. */
function shiftKeys(keys: Record<string, Key[]>, d: Frames): Record<string, Key[]> {
  return Object.fromEntries(Object.entries(keys).map(([p, ks]) => [p, d > 0 ? cutKeys(ks, d)[1] : ks.map((k) => ({ ...k, at: k.at - d }))]))
}

/** A clip as its two parts either side of timeline frame `at`: the left keeps its id, the right takes `id`. Media
 *  keep their place in the source, keys stay at their frames, each fade and each In or Out animation stays on its own end. */
function halves(c: Clip, at: Frames, id: string): [Clip, Clip] {
  const d = at - c.start
  const parts = Object.entries(c.keys ?? {}).map(([p, ks]) => [p, cutKeys(ks, d)] as const)
  const keysOf = (i: 0 | 1) => {
    const ks = Object.fromEntries(parts.filter(([, h]) => h[i].length).map(([p, h]) => [p, h[i]]))
    return Object.keys(ks).length ? ks : undefined
  }
  const share = (len: number) => (c.shows === undefined ? {} : { shows: (c.shows * len) / c.length })
  const left: Clip = { ...c, ...share(d), length: d, fadeOut: undefined, exit: undefined, keys: keysOf(0), markers: c.markers?.filter((m) => m.at < d) }
  const right: Clip = { ...c, ...share(c.length - d), id, start: at, length: c.length - d, fadeIn: undefined, enter: undefined, keys: keysOf(1), markers: c.markers?.filter((m) => m.at >= d).map((m) => ({ ...m, at: m.at - d })), ...(c.in !== undefined && { in: c.in + Math.round(d * speedOf(c)) }) }
  return [left, right]
}

// ---------------------------------------------------------------- settling

/** `next` made whole after `op` changed `was`: the main track closed (the magnet), followers carried (linkage),
 *  tracks in time order, empty made tracks gone, transitions only on cuts that are still there. */
function settle(was: Timeline, next: Timeline, op: Op): Timeline {
  const named = new Set(op.op === "add" ? [op.clip.id] : "ids" in op && op.ids ? op.ids : "id" in op && typeof op.id === "string" ? [op.id] : [])
  let t = next
  if (t.magnet) t = withTracks(t, (k) => (isMain(k) ? { ...k, clips: close(k.clips, op.op === "move" || op.op === "add" ? was : undefined, named) } : k))
  if (was.linkage && t.linkage && op.op !== "option") t = follow(was, t, op, named)
  t = { ...t, tracks: t.tracks.filter((k) => k.clips.length || was.tracks.some((o) => o.id === k.id)).map((k) => ({ ...k, clips: [...k.clips].sort((a, b) => a.start - b.start) })) }
  const cut = (x: Transition) => {
    const at = find(t, x.after)
    return !!at && at.track.clips.some((c) => c.start === end(at.clip))
  }
  return t.transitions.every(cut) ? t : { ...t, transitions: t.transitions.filter(cut) }
}

/** The main track's clips with no gap, from frame 0, in the order of their middles. A clip being moved takes its
 *  place by the edge it leads with, so it swaps with a neighbour once that edge is dragged past the neighbour's
 *  middle (TL4a), as CapCut's main track does. A clip added goes in at the cut nearest where it was put: before
 *  the clip it lands in when that is nearer its start than its end (CapCut's insert at the playhead, TL3j). */
function close(clips: Clip[], was: Timeline | undefined, named: Set<string>): Clip[] {
  const key = (c: Clip) => {
    if (was && named.has(c.id) && !find(was, c.id)) return c.start
    const from = was && named.has(c.id) ? find(was, c.id)?.clip.start : undefined
    if (from === undefined || from === c.start) return c.start + c.length / 2
    return c.start > from ? end(c) : c.start
  }
  let at = 0
  return [...clips]
    .sort((a, b) => key(a) - key(b))
    .map((c) => {
      const moved = c.start === at ? c : { ...c, start: at }
      at += c.length
      return moved
    })
}

/** What sat over a main clip goes where that clip went, and goes when it is removed (TL4c): picture, text and effect
 *  clips whose start was inside its span, unless the operation named them itself. */
function follow(was: Timeline, t: Timeline, op: Op, named: Set<string>): Timeline {
  const mains = was.tracks.filter(isMain).flatMap((k) => k.clips)
  if (!mains.length) return t
  const over = (c: Clip) => mains.find((m) => m.start <= c.start && c.start < end(m))
  const fate = new Map<string, number | "gone">()
  for (const k of was.tracks) {
    if (isMain(k) || k.kind === "sound" || k.locked) continue
    for (const c of k.clips) {
      if (named.has(c.id)) continue
      const m = over(c)
      if (!m) continue
      const now = find(t, m.id)
      if (!now) {
        if (op.op === "remove") fate.set(c.id, "gone")
      } else if (now.clip.start !== m.start && op.op !== "trim" && op.op !== "cut") fate.set(c.id, now.clip.start - m.start)
    }
  }
  if (!fate.size) return t
  return withTracks(t, (k) => ({ ...k, clips: k.clips.filter((c) => fate.get(c.id) !== "gone").map((c) => (typeof fate.get(c.id) === "number" ? { ...c, start: Math.max(0, c.start + (fate.get(c.id) as number)) } : c)) }))
}
