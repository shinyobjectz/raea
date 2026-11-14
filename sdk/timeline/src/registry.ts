// What the timeline offers by name, and what a plugin adds to it (docs/timeline.feature TL11a): easings, speed
// curves and transitions, as data. raea's own come first and can't be replaced; a plugin's are checked before they
// are offered, and one that isn't well formed is left out with why.
import { EASES } from "./keys"
import { type Ease, type SpeedPoint, under } from "./model"
import { CURVES, FASTEST, SLOWEST } from "./speed"
// raea's effects (TL7b): the one table the engine's <Effect> draws from, so a preview here is what a render shows
import { EFFECTS, effectFilter, type EffectKind, type EffectPart, effectPartsError, partsFilter } from "@raea/video/effects"
// raea's Combos (TL7e): the table <Transition combo> draws, a plugin's added to it as data of the same shape
import { COMBOS, type ComboKind } from "@raea/video/edges"

export { EFFECTS, effectFilter, type EffectKind, type EffectPart, partsFilter }

/** An effect the palette offers: raea's own by its kind, a plugin's with the parts that draw it. */
export interface EffectInfo {
  /** its kind, as <Effect kind> takes it */
  kind: string
  /** what the bar and the clip's head say */
  title: string
  /** a plugin's: the CSS filter functions that draw it, each from amount 0 to amount 1 */
  parts?: EffectPart[]
}

export interface TransitionKind {
  /** the component's name in the composition's code */
  kind: string
  /** what the cut's menu says */
  title: string
}

/** A kind of track: what its head says, and where it stacks (with the tracks over the picture, or with the sound
 *  under it). A clip on it is of that kind. */
export interface TrackKindInfo {
  /** its kind, as a Track's kind names it */
  kind: string
  /** what its head says */
  title: string
  /** it stacks with the sound, under the main track */
  under?: boolean
}

/** A way words are put on the picture (TL7c): where they stand, and how they are drawn (CSS, as React writes it). */
export interface TextStyle {
  /** its kind, kept in a text clip's props */
  kind: string
  /** what the bar says */
  title: string
  /** where the words stand on the picture */
  place: "top" | "center" | "bottom"
  /** how they are drawn: type and colour, as React writes CSS (TEXT_PROPS lists what it may set) */
  style: Record<string, string | number>
}
/** The CSS a text style may set: type and colour, nothing that lays the frame out. */
export const TEXT_PROPS = ["color", "fontSize", "fontWeight", "fontFamily", "fontStyle", "letterSpacing", "lineHeight", "textTransform", "textShadow", "background", "padding", "borderRadius", "opacity"]
/** raea's own text styles. Sizes are the composition's pixels (a 1080-line frame). */
export const TEXTS: TextStyle[] = [
  { kind: "title", title: "Title", place: "bottom", style: { color: "white", fontSize: 72, fontWeight: 700, textShadow: "0 2px 16px rgba(0, 0, 0, 0.6)" } },
  { kind: "headline", title: "Headline", place: "center", style: { color: "white", fontSize: 120, fontWeight: 800, letterSpacing: "-0.02em", textShadow: "0 4px 24px rgba(0, 0, 0, 0.5)" } },
  { kind: "caption", title: "Caption", place: "bottom", style: { color: "white", fontSize: 44, fontWeight: 600, background: "rgba(0, 0, 0, 0.6)", padding: "8px 18px", borderRadius: 10 } },
  { kind: "label", title: "Label", place: "top", style: { color: "white", fontSize: 36, fontWeight: 600, letterSpacing: "0.08em", textTransform: "uppercase", textShadow: "0 2px 10px rgba(0, 0, 0, 0.6)" } },
]
/** Why `t` isn't a text style, or undefined when it is. */
export function textStyleError(t: unknown): string | undefined {
  const x = t as Partial<TextStyle> | null
  if (!x || typeof x.kind !== "string" || !/^[a-z][a-z0-9-]*$/.test(x.kind) || typeof x.title !== "string" || !x.title.trim()) return "is { kind, title, place, style }: kind in lowercase letters, digits and dashes, title what the person sees"
  if (x.place !== "top" && x.place !== "center" && x.place !== "bottom") return "place is top, center or bottom"
  if (!x.style || typeof x.style !== "object" || Array.isArray(x.style)) return "style is an object of CSS properties"
  for (const [k, v] of Object.entries(x.style)) {
    if (!TEXT_PROPS.includes(k)) return `style.${k}: a text style sets ${TEXT_PROPS.join(", ")}`
    if (!(typeof v === "number" ? Number.isFinite(v) : typeof v === "string" && v.length <= 120 && !/[{}<>;]|url\(|expression/i.test(v))) return `style.${k} is a number or a plain CSS value`
  }
  return undefined
}

/** A tool a plugin adds (TL11a): one press sets these on every picked clip, as one edit each. */
export interface Preset {
  /** its id: lowercase letters, digits and dashes */
  id: string
  /** what the bar says */
  title: string
  /** what it sets on each picked clip: placement props (PRESET_PROPS), a speed, a level */
  set: { props?: Record<string, number | boolean>; speed?: number; volume?: number }
}
/** Marks edges snap to that a plugin adds (TL11a): a grid, one mark every `every` seconds from the start. */
export interface SnapGrid {
  /** its id: lowercase letters, digits and dashes */
  id: string
  /** what the snap menu says */
  title: string
  /** seconds between its marks, from 0.01 to 600 */
  every: number
}
/** The props a preset may set: a clip's placement (what the editor's own tools set). */
export const PRESET_PROPS = ["x", "y", "scale", "rotate", "opacity", "mirror", "cropLeft", "cropTop", "cropRight", "cropBottom"]
const titled = (x: { id?: unknown; title?: unknown } | null) => !!x && typeof x.id === "string" && /^[a-z][a-z0-9-]*$/.test(x.id) && typeof x.title === "string" && !!x.title.trim()
/** Why `p` isn't a preset, or undefined when it is. */
export function presetError(p: unknown): string | undefined {
  const x = p as Partial<Preset> | null
  if (!titled(x)) return "is { id, title, set }: id in lowercase letters, digits and dashes, title what the person sees"
  const set = x!.set
  if (!set || typeof set !== "object" || Array.isArray(set) || !Object.keys(set).length) return "set is what it puts on a clip: props, speed or volume"
  for (const k of Object.keys(set)) if (!["props", "speed", "volume"].includes(k)) return `set.${k}: a tool sets props, speed or volume`
  if (set.speed !== undefined && !(typeof set.speed === "number" && set.speed >= SLOWEST && set.speed <= FASTEST)) return `set.speed runs from ${SLOWEST} to ${FASTEST}`
  if (set.volume !== undefined && !(typeof set.volume === "number" && set.volume >= 0 && set.volume <= 2)) return "set.volume runs from 0 to 2"
  for (const [k, v] of Object.entries(set.props ?? {})) {
    if (!PRESET_PROPS.includes(k)) return `set.props.${k}: a tool places a clip by ${PRESET_PROPS.join(", ")}`
    if (!(k === "mirror" ? typeof v === "boolean" : typeof v === "number" && Number.isFinite(v))) return `set.props.${k} is ${k === "mirror" ? "true or false" : "a number"}`
  }
  return undefined
}
/** The frames a snap grid marks in a timeline `length` frames long (none for a grid finer than a frame, or of more than 5,000 marks). */
export function gridFrames(grid: SnapGrid, fps: number, length: number): number[] {
  const step = grid.every * fps
  if (!(step >= 1) || length / step > 5000) return []
  return Array.from({ length: Math.floor(length / step) }, (_, i) => Math.round((i + 1) * step))
}

/** What one plugin adds. */
export interface Adds {
  /** easings of its own, by name (TL5c) */
  eases?: Record<string, Ease>
  /** speed curves of its own, by name (TL6b) */
  curves?: Record<string, SpeedPoint[]>
  /** transitions of its own (TL7a) */
  transitions?: TransitionKind[]
  /** Combos of its own (TL7e): how a clip moves over one cycle, as data the timeline's preview draws */
  combos?: ComboKind[]
  /** kinds of track of its own (TL11a): a clip posted or added on one is drawn and edited like any other */
  tracks?: TrackKindInfo[]
  /** effects of its own (TL7b, TL11a), each drawn by its parts: CSS filter functions from amount 0 to amount 1 */
  effects?: EffectInfo[]
  /** text styles of its own (TL7c, TL11a) */
  texts?: TextStyle[]
  /** tools of its own: presets put on the picked clips */
  tools?: Preset[]
  /** marks edges snap to: grids in seconds */
  snaps?: SnapGrid[]
}

/** raea's own kinds of track, in the order they stack (TL1a). */
export const TRACKS: TrackKindInfo[] = [
  { kind: "effect", title: "Effects" },
  { kind: "text", title: "Text" },
  { kind: "overlay", title: "Overlay" },
  { kind: "main", title: "Main" },
  { kind: "sound", title: "Sound", under: true },
]

/** raea's own transitions (TL7a): the kinds its <Transition> draws (sdk/video transition.tsx). Two parts that only
 *  touch have no frames of each other to mix, so there is no dissolve: a fade goes through black. */
export const TRANSITIONS: TransitionKind[] = [
  { kind: "fade", title: "Fade" },
  { kind: "slide", title: "Slide" },
  { kind: "wipe", title: "Wipe" },
  { kind: "zoom", title: "Zoom" },
]

const isEase = (e: unknown): e is Ease => e === "linear" || e === "hold" || (Array.isArray(e) && e.length === 4 && e.every((n) => typeof n === "number" && Number.isFinite(n)) && e[0] >= 0 && e[0] <= 1 && e[2] >= 0 && e[2] <= 1)
const isCurve = (c: unknown): c is SpeedPoint[] =>
  Array.isArray(c) && c.length >= 2 && c.every((p) => p && typeof p.at === "number" && p.at >= 0 && p.at <= 1 && typeof p.speed === "number" && p.speed >= SLOWEST && p.speed <= FASTEST)

const WAVE = ["x", "y", "scale", "rotate", "opacity"]
/** Why a plugin's Combo isn't well formed, or nothing when it is. */
function comboError(x: unknown): string | undefined {
  const c = x as Partial<ComboKind> | null
  if (!c || typeof c.kind !== "string" || !/^[a-z][a-z0-9-]*$/.test(c.kind) || typeof c.title !== "string" || !c.title.trim()) return "is { kind, title, wave }: kind in lowercase letters, digits and dashes, title what the person sees"
  if (!c.wave || typeof c.wave !== "object" || !Object.keys(c.wave).length) return `(${c.kind}): its wave is an object of ${WAVE.join(", ")}`
  for (const [k, v] of Object.entries(c.wave)) {
    if (!WAVE.includes(k)) return `(${c.kind}): a wave moves ${WAVE.join(", ")}, not ${k}`
    if (!Array.isArray(v) || !v.length || v.length > 64 || !v.every((n) => typeof n === "number" && Number.isFinite(n))) return `(${c.kind}): wave.${k} is 1 to 64 numbers over one cycle`
  }
  if (c.linear !== undefined && typeof c.linear !== "boolean") return `(${c.kind}): linear is true or false`
  return undefined
}

/** A plugin's additions as written in its manifest, checked: what is well formed, and why the rest isn't. */
export function readAdds(raw: unknown): { adds: Adds; errors: string[] } {
  const errors: string[] = []
  const adds: Adds = {}
  if (raw === undefined) return { adds, errors }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return { adds, errors: ["timeline is an object: { eases, curves, transitions, combos, tracks, effects, texts, tools, snaps }"] }
  const r = raw as Record<string, unknown>
  const named = (v: unknown, what: string): [string, unknown][] => (v === undefined ? [] : v && typeof v === "object" && !Array.isArray(v) ? Object.entries(v) : (errors.push(`timeline.${what} is an object of named ${what}`), []))
  for (const [name, e] of named(r.eases, "eases")) {
    if (name in EASES) errors.push(`timeline.eases.${name}: raea has an easing of that name`)
    else if (!isEase(e)) errors.push(`timeline.eases.${name} is "linear", "hold" or a bezier's handles [x1, y1, x2, y2] with x1 and x2 from 0 to 1`)
    else (adds.eases ??= {})[name] = e
  }
  for (const [name, c] of named(r.curves, "curves")) {
    if (name in CURVES) errors.push(`timeline.curves.${name}: raea has a speed curve of that name`)
    else if (!isCurve(c)) errors.push(`timeline.curves.${name} is two or more points { at, speed }, at from 0 to 1 and speed from ${SLOWEST} to ${FASTEST}`)
    else (adds.curves ??= {})[name] = c
  }
  if (r.transitions !== undefined) {
    if (!Array.isArray(r.transitions)) errors.push("timeline.transitions is a list of { kind, title }")
    else
      for (const [i, x] of r.transitions.entries()) {
        const t = x as Partial<TransitionKind> | null
        if (!t || typeof t.kind !== "string" || !/^[A-Za-z][\w-]*$/.test(t.kind) || typeof t.title !== "string" || !t.title.trim()) errors.push(`timeline.transitions[${i}] is { kind, title }: kind a name, title what the person sees`)
        else if (TRANSITIONS.some((o) => o.kind === t.kind)) errors.push(`timeline.transitions[${i}]: raea has a transition "${t.kind}"`)
        else (adds.transitions ??= []).push({ kind: t.kind, title: t.title })
      }
  }
  if (r.combos !== undefined) {
    if (!Array.isArray(r.combos)) errors.push("timeline.combos is a list of { kind, title, wave }")
    else
      for (const [i, x] of r.combos.entries()) {
        const why = comboError(x)
        if (why) errors.push(`timeline.combos[${i}] ${why}`)
        else if (COMBOS.some((o) => o.kind === (x as ComboKind).kind)) errors.push(`timeline.combos[${i}]: raea has a combo "${(x as ComboKind).kind}"`)
        else (adds.combos ??= []).push(x as ComboKind)
      }
  }
  if (r.tracks !== undefined) {
    if (!Array.isArray(r.tracks)) errors.push("timeline.tracks is a list of { kind, title }")
    else
      for (const [i, x] of r.tracks.entries()) {
        const t = x as Partial<TrackKindInfo> | null
        if (!t || typeof t.kind !== "string" || !/^[a-z][a-z0-9-]*$/.test(t.kind) || typeof t.title !== "string" || !t.title.trim()) errors.push(`timeline.tracks[${i}] is { kind, title }: kind in lowercase letters, digits and dashes, title what the person sees`)
        else if (TRACKS.some((o) => o.kind === t.kind)) errors.push(`timeline.tracks[${i}]: raea has a track of kind "${t.kind}"`)
        else (adds.tracks ??= []).push({ kind: t.kind, title: t.title, ...(t.under === true && { under: true }) })
      }
  }
  if (r.effects !== undefined) {
    if (!Array.isArray(r.effects)) errors.push("timeline.effects is a list of { kind, title, parts }")
    else
      for (const [i, x] of r.effects.entries()) {
        const e = x as Partial<EffectInfo> | null
        const why = e ? effectPartsError(e.parts) : undefined
        if (!e || typeof e.kind !== "string" || !/^[a-z][a-z0-9-]*$/.test(e.kind) || typeof e.title !== "string" || !e.title.trim()) errors.push(`timeline.effects[${i}] is { kind, title, parts }: kind in lowercase letters, digits and dashes, title what the person sees`)
        else if (EFFECTS.some((o) => o.kind === e.kind)) errors.push(`timeline.effects[${i}]: raea has an effect "${e.kind}"`)
        else if (why) errors.push(`timeline.effects[${i}] (${e.kind}): ${why}`)
        else (adds.effects ??= []).push({ kind: e.kind, title: e.title, parts: e.parts! })
      }
  }
  if (r.tools !== undefined) {
    if (!Array.isArray(r.tools)) errors.push("timeline.tools is a list of { id, title, set }")
    else
      for (const [i, x] of r.tools.entries()) {
        const why = presetError(x)
        if (why) errors.push(`timeline.tools[${i}] ${why}`)
        else (adds.tools ??= []).push(x as Preset)
      }
  }
  if (r.snaps !== undefined) {
    if (!Array.isArray(r.snaps)) errors.push("timeline.snaps is a list of { id, title, every }")
    else
      for (const [i, x] of r.snaps.entries()) {
        const g = x as Partial<SnapGrid> | null
        if (!titled(g) || !(typeof g!.every === "number" && g!.every >= 0.01 && g!.every <= 600)) errors.push(`timeline.snaps[${i}] is { id, title, every }: every the seconds between marks, from 0.01 to 600`)
        else (adds.snaps ??= []).push({ id: g!.id!, title: g!.title!, every: g!.every! })
      }
  }
  if (r.texts !== undefined) {
    if (!Array.isArray(r.texts)) errors.push("timeline.texts is a list of { kind, title, place, style }")
    else
      for (const [i, x] of r.texts.entries()) {
        const why = textStyleError(x)
        if (why) errors.push(`timeline.texts[${i}] ${why}`)
        else if (TEXTS.some((o) => o.kind === (x as TextStyle).kind)) errors.push(`timeline.texts[${i}]: raea has a text style "${(x as TextStyle).kind}"`)
        else (adds.texts ??= []).push(x as TextStyle)
      }
  }
  return { adds, errors }
}

/** Everything offered: raea's own, then each plugin's in order (a name a plugin before it took stays that one's). */
export function palette(plugins: Adds[] = []): { eases: Record<string, Ease>; curves: Record<string, SpeedPoint[]>; transitions: TransitionKind[]; /** the ways a clip moves over its whole length (TL7e) */ combos: ComboKind[]; tracks: TrackKindInfo[]; /** the effects a clip of its own puts on the picture (TL7b) */ effects: EffectInfo[]; /** the ways words are put on the picture (TL7c) */ texts: TextStyle[]; /** a plugin's tools: presets for the picked clips */ tools: Preset[]; /** a plugin's snap grids */ snaps: SnapGrid[] } {
  const eases = { ...EASES }, curves = { ...CURVES }, transitions = [...TRANSITIONS], combos = [...COMBOS], tracks = [...TRACKS]
  const effects: EffectInfo[] = EFFECTS.map(({ kind, title }) => ({ kind, title }))
  const texts = [...TEXTS]
  const tools: Preset[] = [], snaps: SnapGrid[] = []
  for (const p of plugins) {
    for (const [n, e] of Object.entries(p.eases ?? {})) if (!(n in eases)) eases[n] = e
    for (const [n, c] of Object.entries(p.curves ?? {})) if (!(n in curves)) curves[n] = c
    for (const t of p.transitions ?? []) if (!transitions.some((o) => o.kind === t.kind)) transitions.push(t)
    for (const c of p.combos ?? []) if (!combos.some((o) => o.kind === c.kind)) combos.push(c)
    for (const e of p.effects ?? []) if (!effects.some((o) => o.kind === e.kind)) effects.push(e)
    for (const x of p.texts ?? []) if (!texts.some((o) => o.kind === x.kind)) texts.push(x)
    for (const x of p.tools ?? []) if (!tools.some((o) => o.id === x.id)) tools.push(x)
    for (const x of p.snaps ?? []) if (!snaps.some((o) => o.id === x.id)) snaps.push(x)
    for (const t of p.tracks ?? [])
      if (!tracks.some((o) => o.kind === t.kind)) {
        tracks.push(t)
        if (t.under) under.add(t.kind)
      }
  }
  return { eases, curves, transitions, combos, tracks, effects, texts, tools, snaps }
}
