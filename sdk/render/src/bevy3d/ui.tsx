// bevy_ui's Node, Text and TextSpan, drawn by the page: each a box whose CSS gives Blitz's taffy the style bevy_ui's
// convert.rs gives Bevy's, with BackgroundColor, BorderColor, ZIndex and Outline as its props. A Node outside another
// is a root: bevy_ui lays each root in an implicit viewport of its own (a grid the size of the window, its item at the
// start), so a root's box sits in one. Text is Bevy's: its default font (Bevy's FiraMono subset), 20 px (bevy_text's
// rem), white, line height 1.2, wrapped at words, drawn unsmoothed (bevy_text's coverage, not CoreText's growth).
import { Children, createContext, useContext, type CSSProperties, type ReactNode } from "react";
import { fontCss, type FontSource } from "./ui-font";
import {
  ALIGN_CONTENT, ALIGN_ITEMS, ALIGN_SELF, DISPLAY, FLEX_DIRECTION, FLEX_WRAP, JUSTIFY_CONTENT, JUSTIFY_ITEMS, JUSTIFY_SELF, OVERFLOW_AXIS, POSITION_TYPE,
  checkVal, keyword, over, radiusCss, rectCss, type Backdrop, type BorderRadius, type Overflow, type OverflowClipMargin, type UiRect, type Val,
} from "./ui-val";
import { LinearRgba, type Srgba } from "./colors";
import { gradientsCss, gradientsOpaque, type Gradient } from "./ui-gradient";
import { gridCss, type GridAutoFlow, type GridPlacement, type GridTrack, type RepeatedGridTrack } from "./ui-grid";
import { ShadowSamplesCx, shadowCss, transformCss, type BoxShadow, type UiTransform } from "./ui-transform";
import { featuresCss } from "./ui-features";
import { handlersOf, popoverAttr, type PickProps } from "./ui-pick";
import { scrollCss, type IgnoreScroll } from "./ui-scroll";
import { useDebugAttr, type UiDebugOptions } from "./ui-debug";
import type { TransformValue } from "./core";
import { InCanvas3D, eulerOf } from "../3d/model";
import { NativeText2d, type Text2dProps } from "./text2d";
import { useVideoConfig } from "@raea/app/clock";

type Color = Srgba | LinearRgba | string;

/** bevy_ui's Node: its layout fields as Bevy names them (each Bevy's default when left out), and the components that
 *  draw it: background_color (BackgroundColor), border_color (BorderColor, every side), z_index (ZIndex). */
export interface NodeProps extends PickProps {
  /** bevy_ecs's Name, as a Scrollbar's target names its scroller (the element's id) */
  name?: string;
  /** UiDebugOptions: this node's debug overlay, in place of the GlobalUiDebugOptions it's in */
  ui_debug?: UiDebugOptions;
  /** the index its debug outline is coloured by (Hsla::sequential_dispersed), as Bevy colours it by its entity's */
  debug_color_index?: number;
  display?: (typeof DISPLAY)[number];
  box_sizing?: "BorderBox" | "ContentBox";
  position_type?: (typeof POSITION_TYPE)[number];
  left?: Val; right?: Val; top?: Val; bottom?: Val;
  width?: Val; height?: Val; min_width?: Val; min_height?: Val; max_width?: Val; max_height?: Val;
  aspect_ratio?: number;
  flex_direction?: (typeof FLEX_DIRECTION)[number];
  flex_wrap?: (typeof FLEX_WRAP)[number];
  flex_grow?: number; flex_shrink?: number; flex_basis?: Val;
  align_items?: (typeof ALIGN_ITEMS)[number];
  align_self?: (typeof ALIGN_SELF)[number];
  align_content?: (typeof ALIGN_CONTENT)[number];
  justify_content?: (typeof JUSTIFY_CONTENT)[number];
  justify_items?: (typeof JUSTIFY_ITEMS)[number];
  justify_self?: (typeof JUSTIFY_SELF)[number];
  margin?: UiRect; padding?: UiRect; border?: UiRect;
  border_radius?: BorderRadius;
  overflow?: Overflow;
  /** the box its overflow clip clips to, grown by a margin (OverflowClipMargin.border_box().with_margin(25)) */
  overflow_clip_margin?: OverflowClipMargin;
  /** the gutter taffy keeps beside a Scroll axis for its scrollbar, in px (0 by default) */
  scrollbar_width?: number;
  /** IgnoreScroll: the axes it keeps its place on while its parent scrolls */
  ignore_scroll?: IgnoreScroll;
  /** ScrollPosition: how far its Scroll axes are scrolled, in px (kept in range and floored to physical px) */
  scroll_position?: [number, number];
  row_gap?: Val; column_gap?: Val;
  /** the grid's tracks (display Grid): RepeatedGridTracks (RepeatedGridTrack.fr(3, 1)) or GridTracks */
  grid_template_rows?: (RepeatedGridTrack | GridTrack)[]; grid_template_columns?: (RepeatedGridTrack | GridTrack)[];
  /** the implicit tracks' sizes, and how items without a placement flow into them */
  grid_auto_rows?: GridTrack[]; grid_auto_columns?: GridTrack[]; grid_auto_flow?: GridAutoFlow;
  /** where a grid item sits: GridPlacement.start(2), .span(3), ... */
  grid_row?: GridPlacement; grid_column?: GridPlacement;
  background_color?: Color;
  /** BackgroundGradient: gradients over the background colour, the first at the bottom, over the whole node */
  background_gradient?: Gradient[];
  /** BorderGradient: gradients over the border, laid out over the whole node */
  border_gradient?: Gradient[];
  /** BorderColor: every side one colour, or each its own ({ top, right, bottom, left }) */
  border_color?: Color | { top: Color; right: Color; bottom: Color; left: Color };
  /** Outline: a line outside the border box, `offset` from it (0 by default), white by default */
  outline?: { width: Val; offset?: Val; color?: Color };
  z_index?: number;
  /** GlobalZIndex: ranked among the roots and every other GlobalZIndex instead (a non-zero one) */
  global_z_index?: number;
  /** Visibility: Inherited (its parent's, the default), Hidden (laid out, not drawn) or Visible (drawn under a hidden
   *  parent), as CSS's visibility inherits */
  visibility?: "Inherited" | "Hidden" | "Visible";
  /** UiTransform: turned, scaled and moved about its centre after layout */
  ui_transform?: UiTransform;
  /** BoxShadow: drop shadows under it, back to front */
  box_shadow?: BoxShadow;
  /** FixedNode: laid out as a root wherever it is in the tree (its own viewport, the window's), drawn with its parent */
  fixed?: boolean;
  children?: ReactNode;
}

// inside a Node (null at the top: a Node there is a root), and what lies under its content when known
export const InUi = createContext<{ back: Backdrop } | null>(null);
// the UiScale the UI is drawn at: physical px to a logical one (a scale factor of 1)
export const UiScaleCx = createContext(1);

const VALS = ["left", "right", "top", "bottom", "width", "height", "min_width", "min_height", "max_width", "max_height", "flex_basis", "row_gap", "column_gap"] as const;
const CSS_OF: Record<(typeof VALS)[number], keyof CSSProperties> = {
  left: "left", right: "right", top: "top", bottom: "bottom", width: "width", height: "height", min_width: "minWidth", min_height: "minHeight",
  max_width: "maxWidth", max_height: "maxHeight", flex_basis: "flexBasis", row_gap: "rowGap", column_gap: "columnGap",
};

/** A Node's props as the box's CSS: Bevy's defaults made explicit where CSS's differ (display flex, border-box,
 *  position relative, flex-shrink 1, min sizes auto; no font of the page's own), its colours blended as Bevy blends them over
 *  `back`, and the backdrop it leaves its content. */
export function nodeCss(who: string, p: NodeProps, back?: Backdrop, scale = 1): { css: CSSProperties; back: Backdrop; attrs: Record<string, string | number> } {
  const s: Record<string, unknown> = { display: "flex", boxSizing: "border-box", position: "relative", flexShrink: 1, minWidth: "auto", minHeight: "auto" };
  const kw = (k: keyof NodeProps, css: string, allowed: readonly string[]) => {
    const v = p[k];
    if (v === undefined) return;
    const w = keyword(who, k, v as string, allowed);
    if (w !== undefined) s[css] = w === "auto" ? "auto" : w;
  };
  kw("display", "display", DISPLAY);
  kw("position_type", "position", POSITION_TYPE);
  kw("flex_direction", "flexDirection", FLEX_DIRECTION);
  kw("flex_wrap", "flexWrap", FLEX_WRAP);
  kw("align_items", "alignItems", ALIGN_ITEMS);
  kw("align_self", "alignSelf", ALIGN_SELF);
  kw("align_content", "alignContent", ALIGN_CONTENT);
  kw("justify_content", "justifyContent", JUSTIFY_CONTENT);
  kw("justify_items", "justifyItems", JUSTIFY_ITEMS);
  kw("justify_self", "justifySelf", JUSTIFY_SELF);
  if (p.flex_wrap === "NoWrap") s.flexWrap = "nowrap";
  if (p.box_sizing !== undefined) s.boxSizing = keyword(who, "box_sizing", p.box_sizing, ["BorderBox", "ContentBox"]);
  for (const k of VALS) if (p[k] !== undefined) s[CSS_OF[k]] = checkVal(who, k, p[k]);
  for (const k of ["flex_grow", "flex_shrink", "aspect_ratio", "z_index", "global_z_index"] as const) {
    const v = p[k];
    if (v !== undefined && !Number.isFinite(v)) throw new RangeError(`${who}: ${k} is a number, got ${v}`);
  }
  if (p.flex_grow !== undefined) s.flexGrow = p.flex_grow;
  if (p.flex_shrink !== undefined) s.flexShrink = p.flex_shrink;
  if (p.aspect_ratio !== undefined) s.aspectRatio = p.aspect_ratio;
  gridCss(who, p, s);
  if (p.visibility !== undefined && p.visibility !== "Inherited") s.visibility = keyword(who, "visibility", p.visibility, ["Hidden", "Visible"]);
  if (p.ui_transform !== undefined) s.transform = transformCss(who, p.ui_transform);
  if (p.box_shadow !== undefined && p.box_shadow.length) s.boxShadow = shadowCss(who, p.box_shadow);
  // every node a stacking context, so a ZIndex ranks it among its siblings above their parent, as bevy_ui stacks
  // them; a GlobalZIndex ranks it in the root's (raea's Blitz hoists it past every other, layout/paint_tree.rs)
  s.zIndex = p.global_z_index ?? p.z_index ?? 0;
  const samples = useContext(ShadowSamplesCx);
  const attrs: Record<string, string | number> = p.global_z_index ? { "data-raea-bevy-global-z": "" } : {};
  // every shadow drawn as bevy_ui draws it (its spread, its blur summed in BoxShadowSamples steps), raea's Blitz box_shadow.rs
  if (p.box_shadow !== undefined && p.box_shadow.length) attrs["data-raea-bevy-shadow"] = String(samples);
  popoverAttr(who, p, s as Record<string, unknown>, attrs);
  if (p.margin !== undefined) s.margin = rectCss(who, "margin", p.margin);
  if (p.padding !== undefined) s.padding = rectCss(who, "padding", p.padding);
  if (p.border !== undefined) {
    s.borderWidth = rectCss(who, "border", p.border);
    s.borderStyle = "solid";
    s.borderColor = "transparent";
    // bevy_ui draws a background inside the border only (ui.wesl draw_uinode_background): a see-through border
    // shows what's under the node, not its background
    if (!/^(\s*0(px)?)+$/.test(String(s.borderWidth))) s.backgroundClip = "padding-box";
  }
  // (its corners as Bevy resolves them, of its size when that is in px)
  const wh = [p.width, p.height].map((v) => (typeof v === "string" && /^[\d.e+-]+px$/.test(v) ? parseFloat(v) : NaN));
  if (p.border_radius !== undefined) s.borderRadius = radiusCss(who, p.border_radius, wh.every(Number.isFinite) ? (wh as [number, number]) : undefined);
  // (the outline is outside the node, over whatever is there: its siblings too, not only the parent's background, so a
  // translucent one goes as written and the page blends it in linear light, as Bevy does; Color::NONE draws nothing)
  if (p.outline !== undefined) {
    s.outline = `${checkVal(who, "outline.width", p.outline.width)} solid ${over(who, "outline.color", p.outline.color ?? "#ffffff", undefined).css}`;
    if (p.outline.offset !== undefined) s.outlineOffset = checkVal(who, "outline.offset", p.outline.offset);
  }
  // (what lies under the node, which its border blends over: Bevy keeps the background inside the border)
  const under = back;
  if (p.background_color !== undefined) ({ css: s.backgroundColor, back } = over(who, "background_color", p.background_color, back));
  if (p.background_gradient !== undefined) {
    s.backgroundImage = gradientsCss(who, p.background_gradient);
    s.backgroundOrigin = "border-box";
    back = undefined;
  }
  // (Bevy draws a BorderGradient over the border alone, and CSS clips no layer to the border's area: it is the bottom
  // layer over the border box and the node's own gradients sit over it clipped to the padding box, the background
  // colour under both, which is Bevy's picture while those gradients are opaque)
  // (no border, nothing to draw: its layer would show at the background's antialiased edge)
  const bordered = p.border !== undefined && [p.border.left, p.border.right, p.border.top, p.border.bottom].some((v) => parseFloat(String(v)) !== 0);
  if (p.border_gradient !== undefined && bordered) {
    if (p.border_color !== undefined) throw new TypeError(`${who}: border_color with border_gradient is to do; give one`);
    const own = p.background_gradient ?? [];
    if (!gradientsOpaque(who, own)) throw new TypeError(`${who}: border_gradient under a translucent background_gradient is to do`);
    s.backgroundImage = [...(own.length ? [s.backgroundImage] : []), gradientsCss(who, p.border_gradient)].join(", ");
    s.backgroundOrigin = "border-box";
    s.backgroundClip = [...own.map(() => "padding-box"), ...p.border_gradient.map(() => "border-box")].join(", ");
  }
  // (the border over what lies under the node, not its own background, which Bevy draws inside the border only: a
  // translucent border on a translucent background shows one blend, as feathers' disabled switch slide does)
  const bc = p.border_color;
  if (bc !== undefined && typeof bc === "object" && "top" in bc) {
    const side = (k: "top" | "right" | "bottom" | "left") => over(who, `border_color.${k}`, bc[k], under).css;
    s.borderColor = `${side("top")} ${side("right")} ${side("bottom")} ${side("left")}`;
  } else if (bc !== undefined) s.borderColor = over(who, "border_color", bc, under).css;
  if (p.overflow !== undefined) {
    const o = p.overflow;
    for (const a of ["x", "y"] as const) if (!OVERFLOW_AXIS.includes(o?.[a])) throw new TypeError(`${who}: overflow.${a} is one of ${OVERFLOW_AXIS.join(", ")}, got ${JSON.stringify(o?.[a])}`);
    s.overflowX = o.x.toLowerCase();
    s.overflowY = o.y.toLowerCase();
  }
  if (p.name !== undefined) attrs.id = p.name;
  const dbg = useDebugAttr(who, p.ui_debug, p.debug_color_index, useContext(UiScaleCx));
  if (dbg) attrs["data-raea-bevy-debug"] = dbg;
  scrollCss(who, p, s, attrs, scale);
  const m = p.overflow_clip_margin;
  if (m !== undefined) {
    if (!["BorderBox", "PaddingBox", "ContentBox"].includes(m?.visual_box) || !(Number.isFinite(m.margin) && m.margin >= 0)) throw new TypeError(`${who}: overflow_clip_margin is OverflowClipMargin.border_box(), padding_box() or content_box(), with_margin(px >= 0), got ${JSON.stringify(m)}`);
    // CSS's overflow-clip-margin, which raea's Blitz honours for every clipping overflow, as Bevy does
    (s as Record<string, string>).overflowClipMargin = `${m.visual_box.replace("Box", "").toLowerCase()}-box ${m.margin}px`;
  }
  return { css: s as CSSProperties, back, attrs };
}

// a root's implicit viewport (bevy_ui ui_surface.rs): a grid the window's size, its one item at the start of both axes
export const VIEWPORT: CSSProperties = { position: "absolute", left: 0, top: 0, width: "100%", height: "100%", display: "grid", alignItems: "start", justifyItems: "start" };

export function boxOf(css: CSSProperties, back: Backdrop, children: ReactNode, root: boolean | "fixed", font?: ReactNode, attrs?: Record<string, unknown>) {
  const inner = (
    <div style={css} {...attrs}>
      <InUi.Provider value={{ back }}>{children}</InUi.Provider>
    </div>
  );
  // data-raea-blend: a page with bevy_ui in it blends in linear light, as Bevy's UI does (raea-page's paint)
  return root ? <div style={root === "fixed" ? { ...VIEWPORT, position: "fixed" } : VIEWPORT} data-raea-blend="linear">{font}{inner}</div> : <>{font}{inner}</>;
}

/** A box of Bevy's UI, laid out as bevy_ui lays it (taffy, with Bevy's defaults). Put it over the 3D in an
 *  <AbsoluteFill> (after the <Canvas3D>): each root fills the frame as Bevy's window. @bevy bevy_ui::ui_node::Node */
export function Node({ children, fixed, ...p }: NodeProps) {
  const ui = useContext(InUi);
  const n = nodeCss("Node", p, ui?.back, useContext(UiScaleCx));
  return boxOf(n.css, n.back, children, fixed ? "fixed" : ui === null, undefined, { ...n.attrs, ...handlersOf("Node", p) });
}

/** bevy_ui's GhostNode: no box of its own, its children laid out as its parent's (CSS display: contents), each one a
 *  root when it is at the top. @bevy bevy_ui::experimental::GhostNode */
export function GhostNode({ children }: { children?: ReactNode }) {
  return useContext(InUi) === null ? <>{children}</> : <div style={{ display: "contents" }}>{children}</div>;
}

/** bevy_text's TextFont, TextColor, TextLayout and LineHeight, on a Text or a TextSpan. */
export interface TextStyleProps {
  /** FontSource: a font file (staticFile("fonts/FiraSans-Bold.ttf")), FontSource.family(name) or a generic family
   *  (FontSource.serif()); Bevy's default font when left out */
  font?: FontSource;
  /** Bevy's FontSize: a number of px, or { Px }, { Rem } (of bevy_text's 20 px rem), { Vw }, { Vh }, { VMin }, { VMax } */
  font_size?: number | { Px: number } | { Rem: number } | { Vw: number } | { Vh: number } | { VMin: number } | { VMax: number };
  /** FontWeight: 100 to 900 (400 by default) */
  weight?: number;
  style?: "Normal" | "Italic" | "Oblique";
  /** TextFont's width (FontWidth): a share of the face's normal width, FontWidth.CONDENSED 0.75 (1 by default) */
  font_width?: number;
  /** TextColor: white by default */
  color?: Color;
  /** LineHeight: { Px } or { RelativeToFont } (1.2 of the size by default) */
  line_height?: { Px: number } | { RelativeToFont: number };
  /** FontVariations: a variable font's axes by their four-letter tag, { [FontVariationTag.WEIGHT]: 300 } */
  font_variations?: Record<string, number>;
  /** FontFeatures: OpenType features by their four-letter tag, 1 to turn one on, { [FontFeatureTag.SMALL_CAPS]: 1 } */
  font_features?: Record<string, number>;
  /** LetterSpacing: { Px } or { Rem } (of bevy_text's 20 px rem) after every glyph (none by default) */
  letter_spacing?: { Px: number } | { Rem: number };
  /** TextFont's FontSmoothing: "None" draws the glyphs unantialiased (a <Text2d> inside a Canvas3D only) */
  font_smoothing?: "AntiAliased" | "None";
}

/** bevy_text's FontVariationTag: the registered axes' OpenType tags. @bevy bevy_text::FontVariationTag */
export const FontVariationTag = { WEIGHT: "wght", WIDTH: "wdth", SLANT: "slnt", ITALIC: "ital", OPTICAL_SIZE: "opsz" } as const;

function variationsCss(who: string, v: Record<string, number> | undefined): string | undefined {
  if (v === undefined) return undefined;
  const axes = Object.entries(v).map(([tag, n]) => {
    if (!/^[\x20-\x7e]{4}$/.test(tag)) throw new TypeError(`${who}: a font variation's tag is four characters ("wght"), got ${JSON.stringify(tag)}`);
    if (!Number.isFinite(n)) throw new TypeError(`${who}: font variation ${tag} is a number, got ${n}`);
    return `"${tag}" ${n}`;
  });
  return axes.length ? axes.join(", ") : undefined;
}

/** What bevy_text draws with a section's own glyphs (a Text's string, not its spans; a TextSpan's run). */
export interface TextRunProps {
  /** Underline and Strikethrough: a line under or through the run, as thick as the font says */
  underline?: boolean;
  strikethrough?: boolean;
  /** UnderlineColor and StrikethroughColor: the lines' colours (the text's colour by default) */
  underline_color?: Color;
  strikethrough_color?: Color;
  /** TextBackgroundColor: a box behind the run, line by line */
  text_background_color?: Color;
}

export interface TextProps extends TextStyleProps, TextRunProps, NodeProps {
  /** TextLayout's justify (Left by default) and linebreak (WordBoundary) */
  justify?: "Left" | "Center" | "Right" | "Justified" | "Start" | "End";
  linebreak?: "WordBoundary" | "AnyCharacter" | "WordOrCharacter" | "NoWrap";
  /** TextShadow: the glyphs again, unblurred, `offset` px right and down, under the text (TextShadow.default()) */
  shadow?: TextShadow;
}

/** bevy_ui's TextShadow. @bevy bevy_ui::widget::text::TextShadow */
export interface TextShadow { offset: [number, number]; color: Color }
export const TextShadow = {
  /** 4 px right and down, black at 0.75 in linear alpha, as Bevy's default */
  default: (): TextShadow => ({ offset: [4, 4], color: new LinearRgba(0, 0, 0, 0.75) }),
};

/** A section's glyphs in its lines and background: CSS carries a decoration to every element inside it, so a Text's
 *  own are on its strings alone (Bevy's Strikethrough on a Text leaves its spans unstruck). Two colours of line are
 *  two spans, one per text-decoration-color. */
function runOf(who: string, r: TextRunProps, back: Backdrop | undefined, text: ReactNode): ReactNode {
  const col = (k: "underline_color" | "strikethrough_color" | "text_background_color") => (r[k] === undefined ? undefined : over(who, k, r[k]!, back).css);
  const [uc, sc, bg] = [col("underline_color"), col("strikethrough_color"), col("text_background_color")];
  if (!r.underline && !r.strikethrough && bg === undefined) return text;
  const line = (l: string, c: string | undefined): CSSProperties => ({ textDecorationLine: l, textDecorationThickness: "from-font", ...(c ? { textDecorationColor: c } : {}) });
  const one = r.underline && r.strikethrough && uc === sc;
  const outer: CSSProperties = { ...(bg ? { backgroundColor: bg } : {}), ...(one ? line("underline line-through", uc) : r.underline ? line("underline", uc) : !r.underline && r.strikethrough ? line("line-through", sc) : {}) };
  const struck = r.underline && r.strikethrough && !one;
  return <span style={outer}>{struck ? <span style={line("line-through", sc)}>{text}</span> : text}</span>;
}


function sizeCss(who: string, f: TextStyleProps["font_size"]): string {
  if (f === undefined) return "20px";
  if (typeof f === "number") return `${checkSize(who, f)}px`;
  const [k, v] = Object.entries(f)[0] ?? [];
  const unit = { Px: "px", Rem: "rem", Vw: "vw", Vh: "vh", VMin: "vmin", VMax: "vmax" }[k as string];
  if (!unit || typeof v !== "number") throw new TypeError(`${who}: font_size is a number of px or { Px | Rem | Vw | Vh | VMin | VMax }, got ${JSON.stringify(f)}`);
  return unit === "rem" ? `${checkSize(who, v) * 20}px` : `${checkSize(who, v)}${unit}`;
}
function checkSize(who: string, n: number) {
  if (!(n > 0 && Number.isFinite(n))) throw new RangeError(`${who}: font_size is above 0, got ${n}`);
  return n;
}

/** A text style's CSS (every field Bevy's default when left out: a span takes nothing from its Text), and the
 *  @font-face its font needs. */
export function textCss(who: string, p: TextStyleProps, back?: Backdrop): { css: CSSProperties; face: string } {
  const { family, face } = fontCss(who, p.font);
  if (p.weight !== undefined && !(Number.isInteger(p.weight) && p.weight >= 1 && p.weight <= 1000)) throw new RangeError(`${who}: weight is 1 to 1000, got ${p.weight}`);
  const lh = p.line_height;
  let lineHeight: string | number = 1.2;
  if (lh !== undefined) {
    if ("Px" in lh && Number.isFinite(lh.Px)) lineHeight = `${lh.Px}px`;
    else if ("RelativeToFont" in lh && Number.isFinite(lh.RelativeToFont)) lineHeight = lh.RelativeToFont;
    else throw new TypeError(`${who}: line_height is { Px } or { RelativeToFont }, got ${JSON.stringify(lh)}`);
  }
  const fw = p.font_width;
  if (fw !== undefined && !(Number.isFinite(fw) && fw > 0)) throw new RangeError(`${who}: font_width is above 0 (FontWidth.CONDENSED), got ${fw}`);
  const css: CSSProperties = {
    fontFamily: family,
    fontSize: sizeCss(who, p.font_size),
    fontWeight: p.weight ?? 400,
    fontStyle: (p.style ?? "Normal").toLowerCase(),
    color: p.color === undefined ? "#ffffff" : over(who, "color", p.color, back).css,
    lineHeight,
    WebkitFontSmoothing: "antialiased",
  };
  const variations = variationsCss(who, p.font_variations);
  if (variations) css.fontVariationSettings = variations;
  if (fw !== undefined) css.fontStretch = `${fw * 100}%`;
  const features = featuresCss(who, p.font_features);
  if (features) css.fontFeatureSettings = features;
  const ls = p.letter_spacing;
  if (ls !== undefined) {
    const [k, v] = Object.entries(ls)[0] ?? [];
    if (!((k === "Px" || k === "Rem") && Number.isFinite(v))) throw new TypeError(`${who}: letter_spacing is { Px } or { Rem }, got ${JSON.stringify(ls)}`);
    css.letterSpacing = `${k === "Px" ? v : (v as number) * 20}px`;
  }
  return { css, face };
}

const BREAK: Record<NonNullable<TextProps["linebreak"]>, CSSProperties> = {
  WordBoundary: { whiteSpace: "pre-wrap" },
  AnyCharacter: { whiteSpace: "pre-wrap", wordBreak: "break-all" },
  WordOrCharacter: { whiteSpace: "pre-wrap", overflowWrap: "anywhere" },
  NoWrap: { whiteSpace: "pre" },
};

/** Bevy's UI text: a Node whose content is its string and its <TextSpan> children, in its TextFont, TextColor,
 *  TextLayout and LineHeight (Bevy's defaults when left out). @bevy bevy_ui::widget::text::Text */
export function Text({ children, font, font_size, weight, style, font_width, color, line_height, font_variations, font_features, letter_spacing, justify, linebreak, shadow, underline, strikethrough, underline_color, strikethrough_color, text_background_color, font_smoothing, ...node }: TextProps) {
  const ui = useContext(InUi);
  const n = nodeCss("Text", node, ui?.back, useContext(UiScaleCx));
  if (font_smoothing === "None") throw new TypeError("Text: font_smoothing \"None\" is for a <Text2d> inside a Canvas3D (the page's text is always smoothed)");
  const t = textCss("Text", { font, font_size, weight, style, font_width, color, line_height, font_variations, font_features, letter_spacing }, n.back);
  if (justify !== undefined && !["Left", "Center", "Right", "Justified", "Start", "End"].includes(justify)) throw new TypeError(`Text: justify is Left, Center, Right, Justified, Start or End, got ${JSON.stringify(justify)}`);
  if (linebreak !== undefined && !(linebreak in BREAK)) throw new TypeError(`Text: linebreak is WordBoundary, AnyCharacter, WordOrCharacter or NoWrap, got ${JSON.stringify(linebreak)}`);
  const textAlign = { Left: "left", Center: "center", Right: "right", Justified: "justify", Start: "start", End: "end" }[justify ?? "Left"] as CSSProperties["textAlign"];
  // the text itself the Node's own block, as bevy_text's measured leaf: its lines laid out together.
  // data-raea-bevy-text: Blitz measures the block as bevy_text does, rounded up to whole pixels (38.4 px lines are 39).
  // Its own strings carry its lines and background (runOf), inside the block: Blitz draws a line from the run's
  // inline root down, not from a block above it, as thick as the font says (bevy_text's underline_size, not em/10)
  const run = { underline, strikethrough, underline_color, strikethrough_color, text_background_color };
  let own = Children.map(children, (c) => (typeof c === "string" || typeof c === "number" ? runOf("Text", run, n.back, c) : c));
  // an empty Text: bevy_text leaves its empty span out, so parley lays out no text in its own default style, a line
  // 16 px high and 0 wide whatever the Text's font (Bevy 0.20 text_input's empty output box: 16 px inside its padding,
  // at a scale factor of 1). A block that high, not a line, so the Text's own font adds no line box
  if (Children.toArray(children).every((c) => c === "")) own = [<div key="empty" data-raea-empty-text="" style={{ width: 0, height: "16px" }} />];
  const css: CSSProperties = { ...n.css, ...t.css, textAlign };
  if (shadow !== undefined) {
    const [x, y] = shadow.offset ?? [];
    if (!(Number.isFinite(x) && Number.isFinite(y))) throw new TypeError(`Text: shadow is { offset: [x, y], color } (TextShadow.default()), got ${JSON.stringify(shadow)}`);
    css.textShadow = `${x}px ${y}px 0 ${over("Text", "shadow.color", shadow.color, n.back).css}`;
  }
  // the box itself the text's block (a block container, not Node's flex): bevy_ui's Text is a measured leaf, which a
  // grid or a flex line sizes by its text's min- and max-content, and which Blitz sizes as bevy_text does, a wrapped
  // one as wide as its widest line (taffy sizes a flex container's min-content by its items' max-content, so a root
  // Text in its viewport grid never wrapped). Its lines are justified within the box
  if (css.display !== "none") css.display = "flow-root";
  return boxOf({ ...css, ...BREAK[linebreak ?? "WordBoundary"] }, n.back, own, ui === null, <style>{t.face}</style>, { ...n.attrs, "data-raea-bevy-text": "" });
}

/** A run of a <Text> in a style of its own (Bevy's defaults, not its Text's, where left out). @bevy bevy_text::text::TextSpan */
export function TextSpan({ children, underline, strikethrough, underline_color, strikethrough_color, text_background_color, ...p }: TextStyleProps & TextRunProps & { children?: ReactNode }) {
  const back = useContext(InUi)?.back;
  // bevy_text leaves out a span whose size is 0 or less (TextPipeline: "Nothing will be displayed"), its Text's other
  // runs laid out without it
  const size = typeof p.font_size === "number" ? p.font_size : p.font_size && Object.values(p.font_size)[0];
  if (typeof size === "number" && Number.isFinite(size) && size <= 0) return null;
  const t = textCss("TextSpan", p, back);
  const run = { underline, strikethrough, underline_color, strikethrough_color, text_background_color };
  return (
    <>
      <style>{t.face}</style>
      <span style={t.css}>{runOf("TextSpan", run, back, children)}</span>
    </>
  );
}


export type { Text2dProps } from "./text2d";

/** Text in Bevy's 2D world, its block's centre at its Transform (Text2d; Bevy's Anchor::CENTER unless `anchor` says). bevy_text
 *  lays it out as a <Text>'s. Inside a <Canvas3D> it is Bevy's own Text2d (text2d.tsx), ordered with the 2D parts by z;
 *  after it, in the <AbsoluteFill>, it is that Text placed on the frame, over the 2D and 3D parts.
 * @bevy bevy_sprite::text2d::Text2d */
export function Text2d({ transform, justify, anchor = [0, 0], children, bounds, shadow, ...text }: Text2dProps) {
  if (useContext(InCanvas3D)) return <NativeText2d span={TextSpan} transform={transform} justify={justify} anchor={anchor} bounds={bounds} shadow={shadow} {...text}>{children}</NativeText2d>;
  if (bounds || shadow || text.font_smoothing === "None") throw new TypeError("Text2d: bounds, shadow and font_smoothing \"None\" are Bevy's own Text2d's: put it inside the <Canvas3D>");
  if (!(Array.isArray(anchor) && anchor.length === 2 && anchor.every(Number.isFinite))) throw new TypeError(`Text2d: anchor is Bevy's Anchor as [x, y] (-0.5 to 0.5), got ${JSON.stringify(anchor)}`);
  const { width, height } = useVideoConfig();
  const [tx, ty] = transform?.translation ?? [0, 0, 0];
  const [sx, sy] = transform?.scale ?? [1, 1, 1];
  const turn = transform?.rotation ? -eulerOf(transform.rotation)[2] : 0;
  const place: CSSProperties = { position: "absolute", width: "max-content", left: width / 2 + tx, top: height / 2 - ty, transform: `translate(${-(anchor[0] + 0.5) * 100}%, ${-(0.5 - anchor[1]) * 100}%) rotate(${turn}rad) scale(${sx}, ${sy})` };
  return (
    <div style={VIEWPORT}>
      <div style={place}>
        <InUi.Provider value={{ back: undefined }}>
          <Text {...text} justify={justify}>{children}</Text>
        </InUi.Provider>
      </div>
    </div>
  );
}


/** bevy_ui's UiScale: every UI length inside times `value` (2 draws a 16 px node 32 px), as CSS's zoom: laid out
 *  and its text shaped at that size, in physical px, as Bevy lays out (a 3 px padding at 1.25 is 4 px, not 3 drawn 1.25
 *  times as big). Text2d and the 2D and 3D parts aren't UI, so they stay as they are.
 * @bevy bevy_ui::UiScale */
export function UiScale({ value, children }: { value: number; children?: ReactNode }) {
  if (!(Number.isFinite(value) && value > 0)) throw new RangeError(`UiScale: value is above 0, got ${value}`);
  return (
    <div style={{ position: "absolute", left: 0, top: 0, width: "100%", height: "100%", zoom: value }}>
      <UiScaleCx.Provider value={value}>{children}</UiScaleCx.Provider>
    </div>
  );
}

/** The UI camera's viewport (bevy_camera's Viewport on the Camera2d the UI draws through): its roots lay out in this
 *  region of the window, `physical_position` from its top left and `physical_size` big (px, at a scale factor of 1),
 *  and are clipped to it. @bevy bevy_camera::Viewport */
export function Viewport({ physical_position, physical_size, children }: { physical_position: [number, number]; physical_size: [number, number]; children?: ReactNode }) {
  const ok = (v: unknown, min: number) => Array.isArray(v) && v.length === 2 && v.every((n) => Number.isInteger(n) && n >= min);
  if (!ok(physical_position, 0) || !ok(physical_size, 1)) throw new RangeError(`Viewport: physical_position is [x, y] >= 0 and physical_size [w, h] >= 1 in whole px, got ${JSON.stringify({ physical_position, physical_size })}`);
  const [[left, top], [width, height]] = [physical_position, physical_size];
  return <div style={{ position: "absolute", left, top, width, height, overflow: "hidden" }}>{children}</div>;
}

/** The roots inside drawn by their own Camera2d (bevy_ui's UiTargetCamera): a layer over the whole window, stacked by
 *  the camera's `order` as Bevy draws its cameras, lowest first. Cameras on one window share its main texture, which
 *  only the first of them clears (bevy_render's ViewTarget, its first call), so the window's clear is the layered
 *  <Canvas3D clearColor>'s and a later camera's own clear colour shows nowhere. Roots outside any are the default
 *  camera's (the highest order). @bevy bevy_ui::UiTargetCamera */
export function UiTargetCamera({ order = 0, children }: { order?: number; children?: ReactNode }) {
  if (!Number.isInteger(order)) throw new RangeError(`UiTargetCamera: order is a whole number, got ${order}`);
  return <div style={{ position: "absolute", left: 0, top: 0, width: "100%", height: "100%", zIndex: order, isolation: "isolate" }}>{children}</div>;
}
