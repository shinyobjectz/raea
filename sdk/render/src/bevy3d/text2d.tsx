// A <Text2d> inside a <Canvas3D>: Bevy's own Text2d (raea-sys-sprite's text2d.rs), laid out, rasterised and ordered
// among the sprites by bevy_text and bevy_sprite_render, so TextBounds, FontSmoothing::None, a Text2dShadow, its
// spans and a z under a sprite are Bevy's. Sent as a Shape2D row with its `text` spec.
import { Children, isValidElement, type ReactNode } from "react";
import { node3d } from "../3d/core";
import { LinearRgba, Srgba } from "./colors";
import type { TextStyleProps, TextRunProps } from "./ui";
import type { TransformValue } from "./core";
import { useVideoConfig } from "@raea/app/clock";

/** bevy_sprite's Text2dShadow: the glyphs again `offset` px right and up (y up, as the 2D world), under the text;
 *  Text2dShadow::default() is { offset: [4, -4], color: black }. @bevy bevy_sprite::text2d::Text2dShadow */
export interface Text2dShadow {
  offset?: [number, number];
  color?: unknown;
}

export interface Text2dProps extends TextStyleProps, TextRunProps {
  /** its place in Bevy's 2D world: the frame's centre the origin, +Y up, a unit a pixel, turns counterclockwise */
  transform?: TransformValue;
  /** how its lines line up with each other (TextLayout's Justify, Left by default) */
  justify?: "Left" | "Center" | "Right" | "Justified" | "Start" | "End";
  /** TextLayout's LineBreak (WordBoundary by default); inside a Canvas3D */
  linebreak?: "WordBoundary" | "AnyCharacter" | "WordOrCharacter" | "NoWrap";
  /** Bevy's Anchor, as its Vec2: which point of the block sits at its Transform, [-0.5, -0.5] its bottom left to
   * [0.5, 0.5] its top right ([0, 0], the centre, by default; Anchor::TOP_CENTER is [0, 0.5]) */
  anchor?: [number, number];
  /** TextBounds: the box its lines wrap in, [width, height] px (either null, unbounded), the anchor then the box's
   *  point; inside a Canvas3D */
  bounds?: [number | null, number | null];
  /** Text2dShadow (true for its default); inside a Canvas3D */
  shadow?: Text2dShadow | true;
  children?: ReactNode;
}

type Rgba = [number, number, number, number];

function rgba(who: string, c: unknown): Rgba | null {
  if (c === undefined) return null;
  const s = c instanceof LinearRgba ? c.to_srgba() : c instanceof Srgba ? c : typeof c === "string" ? Srgba.hex(c) : undefined;
  if (!s) throw new TypeError(`${who}: a colour is an Srgba, a LinearRgba or a CSS colour, got ${JSON.stringify(c)}`);
  return [s.red, s.green, s.blue, s.alpha];
}

/** One section of the text: its string and its TextFont, colour, background and underline. */
function sectionOf(who: string, text: string, p: TextStyleProps & TextRunProps) {
  const font = p.font;
  if (font !== undefined && typeof font !== "string") throw new TypeError(`${who}: inside a Canvas3D its font is a font file (staticFile("fonts/x.ttf")) or Bevy's default font, got ${JSON.stringify(font)}`);
  const fs = p.font_size;
  const size = fs === undefined ? 20 : typeof fs === "number" ? fs : "Px" in fs ? fs.Px : NaN;
  if (!(Number.isFinite(size) && size > 0)) throw new TypeError(`${who}: inside a Canvas3D its font_size is px (a number or { Px }), got ${JSON.stringify(fs)}`);
  if (p.font_smoothing !== undefined && p.font_smoothing !== "AntiAliased" && p.font_smoothing !== "None") throw new TypeError(`${who}: font_smoothing is "AntiAliased" or "None", got ${JSON.stringify(p.font_smoothing)}`);
  return { text, font: font ?? null, size, weight: p.weight ?? null, color: rgba(who, p.color), background: rgba(who, p.text_background_color), underline: p.underline === true, smoothing: p.font_smoothing ?? "AntiAliased" };
}

/** The Text2d's sections: its own leading text, then each <TextSpan> child in its own style (Bevy's defaults where
 *  left out, as a TextSpan's). `span` is ui.tsx's TextSpan, which marks a span child. */
function sectionsOf(p: Text2dProps, span: unknown) {
  let own = "";
  const spans: ReturnType<typeof sectionOf>[] = [];
  for (const c of Children.toArray(p.children)) {
    if (typeof c === "string" || typeof c === "number") {
      if (spans.length) throw new TypeError("Text2d: inside a Canvas3D its own text comes before its <TextSpan>s");
      own += String(c);
    } else if (isValidElement(c) && c.type === span) {
      const sp = c.props as TextStyleProps & TextRunProps & { children?: ReactNode };
      spans.push(sectionOf("TextSpan", Children.toArray(sp.children).map(String).join(""), sp));
    } else throw new TypeError("Text2d: inside a Canvas3D its children are text and <TextSpan>s");
  }
  return [sectionOf("Text2d", own, p), ...spans];
}

const JUSTIFY = ["Left", "Center", "Right", "Justified", "Start", "End"];
const BREAKS = ["WordBoundary", "AnyCharacter", "WordOrCharacter", "NoWrap"];

/** Bevy's Text2d in the Canvas3D's 2D world: its place the frame's centre the origin, +Y up, a unit a pixel. */
export function NativeText2d({ span, ...p }: Text2dProps & { span: unknown }) {
  const { width, height } = useVideoConfig();
  if (p.justify !== undefined && !JUSTIFY.includes(p.justify)) throw new TypeError(`Text2d: justify is ${JUSTIFY.join(", ")}, got ${JSON.stringify(p.justify)}`);
  if (p.linebreak !== undefined && !BREAKS.includes(p.linebreak)) throw new TypeError(`Text2d: linebreak is ${BREAKS.join(", ")}, got ${JSON.stringify(p.linebreak)}`);
  const b = p.bounds;
  if (b !== undefined && !(Array.isArray(b) && b.length === 2 && b.every((x) => x === null || (Number.isFinite(x) && x > 0)))) throw new TypeError(`Text2d: bounds is TextBounds' [width, height] in px (either null, unbounded), got ${JSON.stringify(b)}`);
  const sh = p.shadow === true ? {} : p.shadow;
  const shadow = sh && { offset: sh.offset ?? [4, -4], color: rgba("Text2d shadow", sh.color) ?? [0, 0, 0, 1] };
  const text = { sections: sectionsOf(p, span), justify: p.justify ?? "Left", linebreak: p.linebreak ?? "WordBoundary", bounds: b ?? null, anchor: p.anchor ?? [0, 0], shadow: shadow ?? null };
  const [tx, ty, tz] = p.transform?.translation ?? [0, 0, 0];
  const [sx, sy] = p.transform?.scale ?? [1, 1, 1];
  const quat = p.transform?.rotation;
  return node3d("Shape2D", { text, x: width / 2 + tx, y: height / 2 - ty, z: tz, scale: [sx, sy], ...(quat ? { quat } : {}) });
}
