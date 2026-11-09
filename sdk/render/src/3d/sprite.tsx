// 2D over the 3D: sprites and flat shapes, drawn over the Canvas3D's 3D by a 2D camera of its own (raea-sys-sprite).
import { Children, isValidElement, useId, type ReactNode } from "react";
import { node3d } from "./core";
import type { Quat } from "../bevy3d/core";
import { samplerSpec, type SamplerSpec } from "./materials";
import { usePointer2d, type Flat, type PointerHandlers3d } from "../events3d";

/** Where a 2D part is: composition px from the frame's top left, y down (as the page's), its centre there; and its
 * pointer and drag handlers, called as a 3D part's are (2D parts are picked first, the top one by z). */
export interface Place2dProps extends PointerHandlers3d {
  name?: string;
  x?: number;
  y?: number;
  /** radians, clockwise (as CSS rotate) */
  rotation?: number;
  /** Bevy's rotation as it is, a quaternion [x, y, z, w] in Bevy's axes (y up, z toward the viewer), in place of
   * `rotation`: a turn out of the frame's plane too, as a Sprite's Transform takes it (camera/2d_on_ui) */
  quat?: Quat;
  /** 1 by default; [x, y] to stretch */
  scale?: number | [number, number];
  /** draw order among the 2D parts: higher draws over lower (0 by default) */
  z?: number;
  /** 0 to 1 (1 by default) */
  opacity?: number;
}

export interface SpriteProps extends Place2dProps {
  /** a picture in the media folder: staticFile("hero.png"), or a "render:NAME" image a camera or a <Compute> draws (a
   * frame may switch it to another by name) */
  src: string;
  /** size in px (the picture's, or one cell's, by default); one given, the other follows the picture's shape */
  width?: number;
  height?: number;
  /** a sprite sheet: the picture is `[columns, rows]` cells of equal size, read left to right, top to bottom */
  grid?: [number, number];
  /** with `grid`: which cell shows (0 is the top left); animate it to play the sheet: frame={Math.floor(f / 3)} */
  frame?: number;
  flipX?: boolean;
  flipY?: boolean;
  /** a tint multiplied over the picture (white by default) */
  color?: string;
  /** Bevy's SpriteImageMode::Tiled: the picture repeats across the sprite's width (tileX) or height (tileY) instead of
   * stretching to it, a tile every `tileStretch` x the picture's size (1 by default): a floor, a fence, a wall. Animate
   * width and height to show more or fewer tiles */
  tileX?: boolean;
  tileY?: boolean;
  tileStretch?: number;
  /** Bevy's SpriteImageMode, as its reflected value: "Auto" (stretched to the size), { Scale: "FillCenter" } (and
   * FillStart, FillEnd, FitCenter, FitStart, FitEnd: the picture kept to its shape, cropped or fitted), { Sliced:
   * TextureSlicer } (nine-slice: border a number of px or { min_inset: [x, y], max_inset: [x, y] }; center_scale_mode
   * and sides_scale_mode "Stretch" or { Tile: { stretch_value } }; max_corner_scale) or { Tiled: { tile_x, tile_y,
   * stretch_value } }. Not with tileX or tileY */
  image_mode?: SpriteImageMode;
  /** the picture's sampler (Bevy's load_with_settings: { filter, mipmap, wrap, anisotropy }), Bevy's default (linear)
   * otherwise; { filter: "nearest" } is pixel art as Bevy's ImagePlugin::default_nearest() draws it */
  sampler?: SamplerSpec;
  /** your own <Material> as its child: Bevy's SpriteMaterial, extending the sprite's own material (the shader reads the
   * sprite's picture, tint and alpha with bevy_sprite_render::sprite_mesh::functions), its uniforms per frame */
  children?: ReactNode;
}

type SliceScaleMode = "Stretch" | { Tile: { stretch_value: number } };
/** Bevy's SpriteImageMode, as its reflected value
 * @bevy bevy_sprite::sprite::SpriteImageMode */
export type SpriteImageMode =
  | "Auto"
  | { Scale: "FillCenter" | "FillStart" | "FillEnd" | "FitCenter" | "FitStart" | "FitEnd" }
  | { Sliced: { border?: number | { min_inset: [number, number]; max_inset: [number, number] }; center_scale_mode?: SliceScaleMode; sides_scale_mode?: SliceScaleMode; max_corner_scale?: number } }
  | { Tiled: { tile_x: boolean; tile_y: boolean; stretch_value: number } };

function checkImageMode(m: unknown): void {
  const bad = () => new TypeError(`Sprite: image_mode is Bevy's SpriteImageMode ("Auto", { Scale: "FillCenter" }, { Sliced: { border: 30 } } or { Tiled: { tile_x, tile_y, stretch_value } }), got ${JSON.stringify(m)}`);
  if (m === "Auto") return;
  if (typeof m !== "object" || m === null || Object.keys(m).length !== 1) throw bad();
  const [k, v] = Object.entries(m)[0] as [string, unknown];
  const o = (typeof v === "object" && v !== null ? v : {}) as Record<string, unknown>;
  const slice = (s: unknown) => s === undefined || s === "Stretch" || Number.isFinite((s as { Tile?: { stretch_value?: number } } | null)?.Tile?.stretch_value);
  const pair = (p: unknown) => Array.isArray(p) && p.length === 2 && p.every(Number.isFinite);
  const border = (b: unknown) => b === undefined || Number.isFinite(b) || (typeof b === "object" && b !== null && pair((b as { min_inset?: unknown }).min_inset) && pair((b as { max_inset?: unknown }).max_inset));
  if (k === "Scale" && ["FillCenter", "FillStart", "FillEnd", "FitCenter", "FitStart", "FitEnd"].includes(v as string)) return;
  if (k === "Sliced" && typeof v === "object" && border(o.border) && slice(o.center_scale_mode) && slice(o.sides_scale_mode) && (o.max_corner_scale === undefined || Number.isFinite(o.max_corner_scale))) return;
  if (k === "Tiled" && typeof o.tile_x === "boolean" && typeof o.tile_y === "boolean" && Number.isFinite(o.stretch_value)) return;
  throw bad();
}

/** A picture in 2D, over the 3D: Bevy's Sprite, drawn as a quad with its sprite material, a sprite sheet's cells cut
 * by a texture atlas (raea-sys-sprite). Put it inside the Canvas3D.
 * @bevy bevy_sprite::sprite::Sprite
 * @bevy bevy_sprite_render::sprite_mesh::sprite_mesh_material::SpriteMeshMaterial
 * @bevy bevy_sprite_render::mesh2d::material::MeshMaterial2d<bevy_sprite_render::sprite_mesh::sprite_mesh_material::SpriteMeshMaterial>
 * @bevy bevy_sprite_render::sprite_mesh::sprite_extended_material::SpriteMaterial
 * @bevy bevy_image::texture_atlas::TextureAtlasLayout */
export function Sprite(props: SpriteProps) {
  if (typeof props.src !== "string" || props.src === "") throw new TypeError("Sprite: src must be a picture (staticFile(\"x.png\"))");
  const gr = props.grid;
  if (gr !== undefined && !(Array.isArray(gr) && gr.length === 2 && gr.every((n) => Number.isInteger(n) && n >= 1))) throw new TypeError(`Sprite: grid is [columns, rows], two whole numbers of 1 or more, got ${JSON.stringify(gr)}`);
  if (props.frame !== undefined && !(Number.isFinite(props.frame) && props.frame >= 0)) throw new RangeError(`Sprite: frame is a cell of the grid, 0 or more, got ${props.frame}`);
  for (const k of ["width", "height"] as const) if (props[k] !== undefined && !(Number.isFinite(props[k]) && props[k] > 0)) throw new RangeError(`Sprite: ${k} is px above 0, got ${props[k]}`);
  if (props.tileStretch !== undefined && !(Number.isFinite(props.tileStretch) && props.tileStretch > 0)) throw new RangeError(`Sprite: tileStretch is above 0, got ${props.tileStretch}`);
  if (props.tileStretch !== undefined && !props.tileX && !props.tileY) throw new TypeError("Sprite: tileStretch is for a tiled sprite: give tileX or tileY");
  if (props.image_mode !== undefined) {
    if (props.tileX || props.tileY) throw new TypeError("Sprite: image_mode or tileX/tileY, not both");
    checkImageMode(props.image_mode);
  }
  if (props.sampler !== undefined) samplerSpec("Sprite: sampler", props.sampler);
  check2d("Sprite", props);
  const id = useId();
  const handled = Object.keys(props).some((k) => /^on[A-Z]/.test(k));
  if (handled && (props.width === undefined || props.height === undefined)) throw new TypeError("Sprite: give width and height to pick it (an on* handler needs its size, which the picture alone doesn't give here)");
  usePointer2d(props.name ?? id, flatOf(props, props.width ?? 0, props.height ?? 0, false), props as unknown as Record<string, unknown>);
  // a <Material> child's spec (the element's type makes it: bevy3d/material.tsx), as a Shape2D's `shader`
  let shader: unknown;
  for (const c of Children.toArray(props.children)) {
    const of = isValidElement(c) ? (c.type as { shaderOf?: (p: unknown) => unknown }).shaderOf : undefined;
    if (!of || shader) throw new TypeError("Sprite: its one child is a <Material> (Bevy's SpriteMaterial)");
    shader = of((c as { props: unknown }).props);
  }
  return node3d("Sprite", { ...props, children: undefined, ...(shader ? { shader } : {}) } as unknown as Record<string, unknown>);
}

export interface Shape2DProps extends Place2dProps {
  /** a rect by default; a polygon has `sides` (5 by default) */
  shape?: "rect" | "circle" | "ellipse" | "triangle" | "polygon";
  /** [width, height] in px ([100, 100] by default); a circle's and a polygon's diameter is the width */
  size?: [number, number];
  sides?: number;
  /** a CSS colour (white by default); with a texture, it tints it */
  color?: string;
  /** the colour as a Bevy Srgba's [r, g, b], unrounded, over `color` (a Mesh2d's ColorMaterial gives it) */
  srgb?: [number, number, number];
  /** a picture on it, mapped by the mesh's UVs (Bevy's ColorMaterial texture), or one with its own sampler (Bevy's
   * load_with_settings: { path, sampler: { wrap: "repeat" } }) */
  texture?: string | { path: string; sampler: SamplerSpec };
  /** the texture's UVs scaled, turned and moved (ColorMaterial's uv_transform, glam's Affine2 as raea/transform's) */
  uv_transform?: { scale: [number, number]; angle: number; translation: [number, number] };
  /** Bevy's AlphaMode2d: "Blend" (Bevy's default), "Opaque", or { Mask: cutoff } */
  alpha_mode?: AlphaMode2d;
  /** one linear [r, g, b, a] per vertex of the shape's mesh (Bevy's Mesh::ATTRIBUTE_COLOR): the material's colour and
   * texture are multiplied by them. A rect has 4 (top right, top left, bottom left, bottom right), a triangle 3. */
  vertex_colors?: [number, number, number, number][];
  /** its own Material2d in place of the ColorMaterial: the spec a <Material> in a Mesh2d makes (docs/control.feature
   * FC2a); the shape's colour, texture and alpha_mode are then the material's to draw */
  shader?: Record<string, unknown>;
  /** one of Bevy's 2D primitives in place of `shape`, its reflected fields over Bevy's default: { primitive:
   * "Capsule2d", radius: 25, half_length: 25 }; a 2D shape given `inner` (the same shape's fields) is Bevy's Ring, or
   * given `ring: thickness` its to_ring (raea-sys-mesh primitive_mesh). A Mesh2d's primitive child makes it. */
  primitive?: Record<string, unknown>;
}

/** bevy_sprite_render's AlphaMode2d */
export type AlphaMode2d = "Opaque" | "Blend" | { Mask: number };

/** A flat shape in 2D, over the 3D, in one colour: Bevy's Mesh2d with a ColorMaterial (raea-sys-sprite). Put it
 * inside the Canvas3D.
 * @bevy bevy_sprite_render::mesh2d::color_material::ColorMaterial
 * @bevy bevy_sprite_render::mesh2d::material::MeshMaterial2d<bevy_sprite_render::mesh2d::color_material::ColorMaterial>
 * @bevy bevy_mesh::components::Mesh2d */
export function Shape2D(props: Shape2DProps) {
  const shapes = ["rect", "circle", "ellipse", "triangle", "polygon"];
  if (props.shape !== undefined && !shapes.includes(props.shape)) throw new TypeError(`Shape2D: shape is one of ${shapes.join(", ")}, got ${JSON.stringify(props.shape)}`);
  const s = props.size;
  if (s !== undefined && !(Array.isArray(s) && s.length === 2 && s.every((n) => Number.isFinite(n) && n > 0))) throw new TypeError(`Shape2D: size is [width, height], two numbers above 0, got ${JSON.stringify(s)}`);
  if (props.sides !== undefined && !(Number.isInteger(props.sides) && props.sides >= 3)) throw new RangeError(`Shape2D: sides is a whole number of 3 or more, got ${props.sides}`);
  check2d("Shape2D", props);
  const tx = props.texture;
  const sampled = typeof tx === "object" && tx !== null && typeof tx.path === "string" && tx.path !== "";
  if (sampled) samplerSpec("Shape2D: texture", tx.sampler);
  const u = props.uv_transform;
  if (u !== undefined && !(Array.isArray(u.scale) && u.scale.length === 2 && u.scale.every(Number.isFinite) && Number.isFinite(u.angle) && Array.isArray(u.translation) && u.translation.length === 2 && u.translation.every(Number.isFinite))) throw new TypeError(`Shape2D: uv_transform is an Affine2 ({ scale: [x, y], angle, translation: [x, y] }), got ${JSON.stringify(u)}`);
  if (tx !== undefined && !sampled && !(typeof tx === "string" && tx !== "")) throw new TypeError(`Shape2D: texture is a picture's path, staticFile("icon.png"), or { path, sampler }, got ${JSON.stringify(props.texture)}`);
  const a = props.alpha_mode;
  if (a !== undefined && a !== "Opaque" && a !== "Blend" && !(typeof a === "object" && a !== null && typeof a.Mask === "number" && Number.isFinite(a.Mask))) throw new TypeError(`Shape2D: alpha_mode is "Opaque", "Blend" or { Mask: cutoff }, got ${JSON.stringify(a)}`);
  const vc = props.vertex_colors;
  if (vc !== undefined && !(Array.isArray(vc) && vc.every((c) => Array.isArray(c) && c.length === 4 && c.every(Number.isFinite)))) throw new TypeError(`Shape2D: vertex_colors is one linear [r, g, b, a] per vertex, got ${JSON.stringify(vc)}`);
  const verts = ({ rect: 4, triangle: 3 } as Record<string, number>)[props.shape ?? "rect"];
  if (vc !== undefined && verts !== undefined && props.primitive === undefined && vc.length !== verts) throw new RangeError(`Shape2D: a ${props.shape ?? "rect"}'s mesh has ${verts} vertices, so vertex_colors has ${verts} colours, got ${vc.length}`);
  const id = useId();
  const [w, h] = props.size ?? [100, 100];
  const round = props.shape === "circle" || props.shape === "ellipse" || props.shape === "polygon";
  // a circle's and a polygon's diameter is the width; a polygon is picked as its circle
  usePointer2d(props.name ?? id, flatOf(props, w, props.shape === "circle" || props.shape === "polygon" ? w : h, round), props as unknown as Record<string, unknown>);
  return node3d("Shape2D", props as unknown as Record<string, unknown>);
}

/** A 2D part's place for picking: its size times its scale, about its centre. */
function flatOf(p: Place2dProps, w: number, h: number, round: boolean): Flat {
  const [sx, sy] = Array.isArray(p.scale) ? p.scale : [p.scale ?? 1, p.scale ?? 1];
  return { x: p.x ?? 0, y: p.y ?? 0, w: w * sx, h: h * sy, rotation: p.rotation ?? 0, z: p.z ?? 0, round };
}

function check2d(owner: string, p: Place2dProps): void {
  for (const k of ["x", "y", "rotation", "z"] as const) if (p[k] !== undefined && !Number.isFinite(p[k])) throw new RangeError(`${owner}: ${k} must be a number, got ${p[k]}`);
  const q = p.quat;
  if (q !== undefined && !(Array.isArray(q) && q.length === 4 && q.every(Number.isFinite) && Math.abs(Math.hypot(...q) - 1) < 1e-3)) throw new TypeError(`${owner}: quat is a unit quaternion [x, y, z, w], got ${JSON.stringify(q)}`);
  if (q !== undefined && p.rotation !== undefined) throw new TypeError(`${owner}: give rotation or quat, not both`);
  if (p.opacity !== undefined && !(Number.isFinite(p.opacity) && p.opacity >= 0 && p.opacity <= 1)) throw new RangeError(`${owner}: opacity is 0 to 1, got ${p.opacity}`);
  const sc = p.scale;
  if (sc !== undefined && !(Number.isFinite(sc) || (Array.isArray(sc) && sc.length === 2 && sc.every(Number.isFinite)))) throw new TypeError(`${owner}: scale is a number or [x, y], got ${JSON.stringify(sc)}`);
}
