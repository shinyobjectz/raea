// Text in the 3D world (docs/ts-native.feature P19).
import { Children, isValidElement, type ReactNode } from "react";
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { toHex } from "../colour";
import { reverse, sampleMove, type MoveLike } from "@raea/math/animate";
import { Vec3, checkVec, node3d } from "./all";
import { checkSequenceFrames } from "./clock";

// ---- text in the 3D world (docs/ts-native.feature P19) ----

export interface SpanProps {
  /** the run's text */
  children?: ReactNode;
  /** a family (raea's bundled faces: Inter, Montserrat, Oswald, Playfair Display...; or one on this machine) */
  font?: string;
  /** 100..900 (the block's by default) */
  weight?: number;
  /** slanted (the block's by default) */
  italic?: boolean;
  /** times the block's size */
  size?: number;
  /** the run's colour (the block's by default) */
  color?: string;
}

/** A run of a <RichText>'s text in its own font, weight, size or colour. Only inside a <RichText>. */
export function Span(_props: SpanProps): null {
  return null;
}

/** What a glyph is, for a <RichText>'s `glyph` function. */
export interface GlyphInfo {
  /** its place among the drawn glyphs (spaces aren't glyphs), from 0 */
  index: number;
  /** how many glyphs the block has */
  count: number;
  /** the character it draws */
  char: string;
  /** its word's place among the words, from 0, and how many there are */
  word: number;
  /** how many words the block has */
  words: number;
  /** its place in its word, and the word's glyph count */
  inWord: number;
  /** how many glyphs its word has */
  wordLength: number;
  /** its line (by the newlines in the text), from 0 */
  line: number;
  /** its run: the <Span> it's in (0 for text outside any, counted in order) */
  span: number;
}

/** A glyph's motion this frame, about its own centre, in the block's space: added to (position, rotation),
 * multiplied into (scale, opacity) or replacing (color, visible) what the block's moves give it. */
export interface GlyphStyle {
  /** metres, from its place in the block */
  position?: Vec3;
  /** radians, XYZ */
  rotation?: Vec3;
  /** multiplies its size: one number, or x, y and z */
  scale?: number | Vec3;
  /** replaces its colour */
  color?: string;
  /** multiplies its opacity, 0..1 */
  opacity?: number;
  /** false hides it this frame */
  visible?: boolean;
}

export interface RichTextProps {
  /** what it's called across frames, and its glyphs' names over world.* (`${name}:${index}`) */
  name?: string;
  /** false: scenery, not words to read (a shop sign seen in passing, a page in a book), so raea render doesn't hold it
   * to type that reads: its size on screen, its angle to the camera, its reveal and its reading time (SG10) */
  read?: boolean;
  /** the composition pass it draws in ("scene" by default): a frame-wide effect told to skip a pass (<Look skip>,
   * <Outline skip>, <Bloom skip>, <Grade skip>) leaves its pixels as drawn, and <Look passes> gives one its own paint;
   * a piece names at most three passes besides "scene"; keyed per frame */
  pass?: string;
  /** where the block's origin is, metres (x, y, z) */
  position?: Vec3;
  /** the block's rotation, radians (x, y, z) */
  rotation?: Vec3;
  /** the block's scale: one number, or x, y and z */
  scale?: number | Vec3;
  /** the em in metres (0.5 by default) */
  size?: number;
  /** a family (raea's bundled faces: Inter by default, Montserrat, Oswald, Playfair Display...; or one on this machine) */
  font?: string;
  /** 100..900 (700 by default) */
  weight?: number;
  /** slanted (false by default) */
  italic?: boolean;
  /** each glyph's colour unless its <Span> gives one */
  color?: string;
  /** where the origin is: the left edge, the middle (default) or the right edge of the lines; the block's middle
   * vertically */
  align?: "left" | "center" | "right";
  /** times the em (1.2 by default) */
  lineHeight?: number;
  /** wrapped at this width in metres (no wrapping by default) */
  maxWidth?: number;
  /** 0..1, smooth to matte */
  roughness?: number;
  /** 0..1, a dielectric to a metal */
  metalness?: number;
  /** the colour it glows with */
  emissive?: string;
  /** how strongly it glows (1 by default; 2-10 with bloom) */
  emissiveIntensity?: number;
  /** drawn in its flat colour, without light or shade */
  unlit?: boolean;
  /** letter-shaped shadows (true by default) */
  castShadow?: boolean;
  /** other things' shadows fall on it */
  receiveShadow?: boolean;
  /** the render layers it's on ([0] by default): a camera draws only the layers it's given */
  layers?: number[];
  /** each glyph's (or word's) entrance: a move's name, a move or several (rise, pop, slam, slide, fade... the 2D ones,
   * their px read at 100 an em, their degrees as degrees), `each` frames apart from `at` */
  enter?: MoveLike;
  /** frames between one glyph's (or word's) entrance and the next (2 by default) */
  each?: number;
  /** the entrance and exit staggered by letter (default) or by word */
  by?: "letter" | "word";
  /** the local frame the first entrance starts (0) */
  at?: number;
  /** each glyph's (or word's) exit: a move or its name, played backwards from `exitAt` */
  exit?: MoveLike;
  /** the local frame the first exit starts; an exit needs it */
  exitAt?: number;
  /** frames between exits (0: together) */
  exitEach?: number;
  /** how many glyphs are shown, a typewriter's count: the next one fades in over its fraction */
  reveal?: number;
  /** each glyph's own motion this frame, from anything the frame gives (useCurrentFrame, spring, interpolate) */
  glyph?: (g: GlyphInfo) => GlyphStyle;
  /** the text: strings and <Span> runs */
  children?: ReactNode;
}

type RunSpec = { t: string; font: string; weight: number; italic: boolean; size: number; color: string };

function textRuns(owner: string, p: RichTextProps): RunSpec[] {
  const base = { font: p.font ?? "Inter", weight: p.weight ?? 700, italic: p.italic ?? false, size: 1, color: toHex(owner, "color", p.color ?? "#ffffff") };
  const out: RunSpec[] = [];
  const text = (n: ReactNode): string => {
    let t = "";
    Children.forEach(n, (c) => {
      if (typeof c === "string" || typeof c === "number") t += String(c);
      else if (isValidElement(c)) throw new TypeError(`${owner}: a <Span> holds text only`);
    });
    return t;
  };
  Children.forEach(p.children, (c) => {
    if (typeof c === "string" || typeof c === "number") {
      out.push({ ...base, t: String(c) });
    } else if (isValidElement(c) && c.type === Span) {
      const sp = c.props as SpanProps;
      if (sp.size !== undefined && !(sp.size > 0)) throw new RangeError(`${owner}: a Span's size is a positive multiple of the block's, got ${sp.size}`);
      out.push({ t: text(sp.children), font: sp.font ?? base.font, weight: sp.weight ?? base.weight, italic: sp.italic ?? base.italic, size: sp.size ?? 1, color: sp.color !== undefined ? toHex(owner, "color", sp.color) : base.color });
    } else if (c !== null && c !== undefined && typeof c !== "boolean") {
      throw new TypeError(`${owner}: its children are text and <Span>s`);
    }
  });
  if (!out.some((r) => r.t.trim() !== "")) throw new TypeError(`${owner}: give it some text`);
  return out;
}

const utf8 = (c: string) => { const n = c.codePointAt(0)!; return n < 0x80 ? 1 : n < 0x800 ? 2 : n < 0x10000 ? 3 : 4; };

/** Text in the 3D world: every glyph a real lit, shaded entity (bevy_rich_text3d's mesh over its glyph atlas, a
 * StandardMaterial of its own), laid out as one line of shaped text (kerning, spans of other fonts and sizes) and
 * then moved, turned, scaled, recoloured, faded and revealed glyph by glyph with the motion code 2D uses: the named
 * moves staggered (`enter="rise" each={2}`), a typewriter's `reveal`, or a `glyph` function of the frame.
 *
 *   <RichText name="title" size={0.6} enter="pop" each={3}>Hello <Span color="#ffd400" weight={900}>world</Span></RichText>
 *   <RichText glyph={(g) => ({ position: [0, Math.sin(frame / 6 + g.index) * 0.05, 0] })}>WAVE</RichText>
 */
export function RichText(props: RichTextProps) {
  const owner = "RichText";
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const { position = [0, 0, 0], rotation = [0, 0, 0], scale = 1, size = 0.5, align = "center", each = 2, by = "letter", at = 0, exitEach = 0 } = props;
  checkVec(owner, "position", position);
  checkVec(owner, "rotation", rotation);
  if (!(size > 0) || !Number.isFinite(size)) throw new RangeError(`${owner}: size is the em in metres, a positive number, got ${size}`);
  if (!["left", "center", "right"].includes(align)) throw new RangeError(`${owner}: align is left, center or right, got ${JSON.stringify(align)}`);
  if (props.exit !== undefined && props.exitAt === undefined) throw new Error(`${owner}: an exit needs exitAt (the local frame it starts)`);
  // one clock (docs/stage-guards.feature SG5): `at` and `exitAt` are frames of this Sequence
  checkSequenceFrames(owner, props.name, [["at", props.at], ["exitAt", props.exitAt]], durationInFrames);
  const runs = textRuns(owner, props);
  // the glyphs: every character that isn't a space, with its byte in the joined text (raea-render lays the text out whole)
  type G = { ch: string; at: number; span: number; color: string; line: number; word: number; inWord: number };
  const gs: G[] = [];
  let byte = 0, line = 0, word = -1, inWord = 0, gap = true;
  runs.forEach((r, span) => {
    for (const ch of r.t) {
      if (ch === "\n") line++;
      if (/\s/.test(ch)) gap = true;
      else {
        if (gap) { word++; inWord = 0; gap = false; }
        gs.push({ ch, at: byte, span, color: r.color, line, word, inWord: inWord++ });
      }
      byte += utf8(ch);
    }
  });
  const words = word + 1;
  const wordLength = (w: number) => gs.filter((g) => g.word === w).length;
  const glyphs = gs.map((g, index) => {
    let p: Vec3 = [0, 0, 0], r: Vec3 = [0, 0, 0], s: Vec3 = [1, 1, 1], o = 1, c = g.color, v = true;
    const apply = (vals: Record<string, number | string>) => {
      const k = size * runs[g.span].size / 100;
      const num = (x: number | string | undefined, d: number) => (typeof x === "number" ? x : d);
      p = [p[0] + num(vals.x, 0) * k, p[1] - num(vals.y, 0) * k, p[2] + num(vals.z, 0) * k];
      const deg = Math.PI / 180;
      r = [r[0] + num(vals.rotateX, 0) * deg, r[1] + num(vals.rotateY, 0) * deg, r[2] - num(vals.rotate ?? vals.rotateZ, 0) * deg];
      const sc = num(vals.scale, 1);
      s = [s[0] * sc * num(vals.scaleX, 1), s[1] * sc * num(vals.scaleY, 1), s[2] * sc];
      o *= num(vals.opacity, 1);
      if (typeof vals.color === "string") c = toHex(owner, "color", vals.color);
    };
    const step = by === "word" ? g.word : index;
    if (props.enter !== undefined) apply(sampleMove(props.enter, at + step * each, frame, fps));
    if (props.exit !== undefined) apply(sampleMove(reverseMove(props.exit), props.exitAt! + step * exitEach, frame, fps));
    if (props.reveal !== undefined) o *= Math.min(1, Math.max(0, props.reveal - index));
    if (props.glyph) {
      const own = props.glyph({ index, count: gs.length, char: g.ch, word: g.word, words, inWord: g.inWord, wordLength: wordLength(g.word), line: g.line, span: g.span }) ?? {};
      if (own.position) { checkVec(owner, "a glyph's position", own.position); p = [p[0] + own.position[0], p[1] + own.position[1], p[2] + own.position[2]]; }
      if (own.rotation) { checkVec(owner, "a glyph's rotation", own.rotation); r = [r[0] + own.rotation[0], r[1] + own.rotation[1], r[2] + own.rotation[2]]; }
      if (own.scale !== undefined) { const t = typeof own.scale === "number" ? [own.scale, own.scale, own.scale] : own.scale; s = [s[0] * t[0], s[1] * t[1], s[2] * t[2]]; }
      if (own.opacity !== undefined) o *= own.opacity;
      if (own.color !== undefined) c = toHex(owner, "a glyph's color", own.color);
      if (own.visible === false) v = false;
    }
    const r6 = (x: number) => Math.round(x * 1e6) / 1e6;
    return { at: g.at, span: g.span, ch: g.ch, p: p.map(r6), r: r.map(r6), s: s.map(r6), c, o: r6(Math.min(1, Math.max(0, o))), ...(v ? {} : { v: false }) };
  });
  const { children: _c, glyph: _g, enter: _e, exit: _x, ...rest } = props;
  return node3d("RichText", {
    ...rest,
    position, rotation, scale, size,
    align: align === "left" ? 0 : align === "right" ? 1 : 0.5,
    ...(props.emissive !== undefined ? { emissive: toHex(owner, "emissive", props.emissive) } : {}),
    text: runs.map((r) => r.t).join(""),
    spans: runs.map(({ t, font, weight, italic, size }) => ({ t, font, weight, italic, size })),
    glyphs,
  });
}

/** A move played backwards (an entrance as its exit), as <Animate exit> plays it. */
function reverseMove(m: MoveLike): MoveLike {
  return reverse(m);
}
