// Bevy's Mesh2d (docs/sdk.feature S2a): a 2D mesh of one of bevy_math's 2D primitives (Circle, Capsule2d, a Ring and the
// rest) with a ColorMaterial, placed by its Transform as Bevy's 2D camera sees it: the frame's centre is the
// origin, +Y up, a unit a pixel, turns counterclockwise. It draws as <Shape2D> does (raea-sys-sprite).
import { Children, isValidElement, type ReactNode } from "react";
import { useVideoConfig } from "@raea/app/clock";
import { Shape2D, type AlphaMode2d, type Place2dProps, type Shape2DProps } from "../3d/sprite";
import { eulerOf } from "../3d";
import { LinearRgba, Srgba, type Affine2, type SampledImage, type TransformValue } from "./all";
import { Material, materialShader, type MaterialProps } from "./material";

/** a primitive's mesh attributes: its Mesh::ATTRIBUTE_COLOR, one linear colour per vertex (as mesh.insert_attribute) */
export interface MeshAttributes { vertex_colors?: (LinearRgba | [number, number, number, number])[] }
/** a 2D shape as Bevy's Ring: of it and `inner` (the same shape's fields), or its to_ring(`ring`, a thickness) */
export interface RingProps { ring?: number; inner?: Record<string, unknown> }
type Vec2 = [number, number];
export interface CircleProps extends MeshAttributes, RingProps { radius?: number }
export interface RectangleProps extends MeshAttributes, RingProps { half_size?: Vec2 }
export interface EllipseProps extends MeshAttributes, RingProps { half_size?: Vec2 }
export interface RegularPolygonProps extends MeshAttributes, RingProps { circumcircle?: { radius: number }; sides?: number }
/** an arc: its radius and half its angle in radians (Bevy's CircularSector::new(r, a) gives half_angle a) */
export interface ArcProps extends MeshAttributes, RingProps { arc?: { radius: number; half_angle: number } }
export interface AnnulusProps extends MeshAttributes, RingProps { inner_circle?: { radius: number }; outer_circle?: { radius: number } }
export interface Capsule2dProps extends MeshAttributes, RingProps { radius?: number; half_length?: number }
export interface RhombusProps extends MeshAttributes, RingProps { half_diagonals?: Vec2 }
export interface VerticesProps extends MeshAttributes, RingProps { vertices?: Vec2[] }
export interface ColorMaterialProps {
  /** white by default; it tints the texture, and its alpha is the material's (Bevy's Srgba or LinearRgba, or a hex) */
  color?: string | Srgba | LinearRgba;
  /** a picture, by the mesh's UVs, or one with its own sampler ({ path, sampler }, Bevy's load_with_settings) */
  texture?: string | SampledImage;
  /** the texture's UVs scaled, turned and moved (glam's Affine2: Affine2.fromScale([2, 3])) */
  uv_transform?: Affine2;
  /** Bevy's AlphaMode2d, "Blend" by default */
  alpha_mode?: AlphaMode2d;
}

const inMesh2d = (name: string) => (): ReactNode => {
  throw new TypeError(`${name} goes in a Mesh2d: <Mesh2d><${name} /><ColorMaterial color="#c84" /></Mesh2d>`);
};
/** bevy_math's Circle (radius 0.5 by default, as Bevy's) */
export const Circle = inMesh2d("Circle") as (p: CircleProps) => ReactNode;
/** bevy_math's Rectangle (half_size [0.5, 0.5] by default) */
export const Rectangle = inMesh2d("Rectangle") as (p: RectangleProps) => ReactNode;
/** bevy_math's Ellipse (half_size [1, 0.5] by default) */
export const Ellipse = inMesh2d("Ellipse") as (p: EllipseProps) => ReactNode;
/** bevy_math's RegularPolygon (circumradius 0.5, 6 sides by default) */
export const RegularPolygon = inMesh2d("RegularPolygon") as (p: RegularPolygonProps) => ReactNode;
/** bevy_math's other 2D primitives, their fields as Bevy reflects them (each over Bevy's default) */
export const CircularSector = inMesh2d("CircularSector") as (p: ArcProps) => ReactNode;
export const CircularSegment = inMesh2d("CircularSegment") as (p: ArcProps) => ReactNode;
export const Annulus = inMesh2d("Annulus") as (p: AnnulusProps) => ReactNode;
export const Capsule2d = inMesh2d("Capsule2d") as (p: Capsule2dProps) => ReactNode;
export const Rhombus = inMesh2d("Rhombus") as (p: RhombusProps) => ReactNode;
export const Triangle2d = inMesh2d("Triangle2d") as (p: VerticesProps) => ReactNode;
export const Segment2d = inMesh2d("Segment2d") as (p: VerticesProps) => ReactNode;
export const Polyline2d = inMesh2d("Polyline2d") as (p: VerticesProps) => ReactNode;
export const ConvexPolygon = inMesh2d("ConvexPolygon") as (p: VerticesProps) => ReactNode;
/** each primitive by its Bevy name */
const PRIMITIVES = new Map<unknown, string>([[Circle, "Circle"], [Rectangle, "Rectangle"], [Ellipse, "Ellipse"], [RegularPolygon, "RegularPolygon"], [CircularSector, "CircularSector"], [CircularSegment, "CircularSegment"], [Annulus, "Annulus"], [Capsule2d, "Capsule2d"], [Rhombus, "Rhombus"], [Triangle2d, "Triangle2d"], [Segment2d, "Segment2d"], [Polyline2d, "Polyline2d"], [ConvexPolygon, "ConvexPolygon"]]);
/** bevy_sprite_render's ColorMaterial: a colour (white by default), a texture it tints, and its AlphaMode2d */
export const ColorMaterial = inMesh2d("ColorMaterial") as (p: ColorMaterialProps) => ReactNode;

export interface Mesh2dProps extends Omit<Place2dProps, "x" | "y" | "rotation" | "scale" | "z"> {
  transform?: TransformValue;
  visibility?: "Inherited" | "Visible" | "Hidden";
  /** one of bevy_math's 2D primitives (<Circle>, <Capsule2d>, ...) and a <ColorMaterial>, or your own <Material>
   *  (Bevy's Material2d, docs/control.feature FC2a) */
  children?: ReactNode;
}

type ShapeOf = Pick<Shape2DProps, "shape" | "size" | "sides" | "vertex_colors" | "primitive">;

/** The primitive's Shape2D, with its vertex colours as linear [r, g, b, a]. */
function shapeOf(el: { type: unknown; props: unknown }): ShapeOf {
  const vc = (el.props as MeshAttributes).vertex_colors;
  const s = primitiveOf(el);
  return vc === undefined ? s : { ...s, vertex_colors: vc.map((c) => (c instanceof LinearRgba ? [c.red, c.green, c.blue, c.alpha] : c)) };
}

/** The primitive's shape, size in px and sides. */
function primitiveOf(el: { type: unknown; props: unknown }): ShapeOf {
  // a shape Shape2D has no name for, or a ring: Bevy's mesher builds it (raea-sys-mesh primitive_mesh)
  const { vertex_colors: _, ...fields } = el.props as MeshAttributes & Record<string, unknown>;
  const simple = el.type === Circle || el.type === Rectangle || el.type === Ellipse || el.type === RegularPolygon;
  if (!simple || fields.ring !== undefined || fields.inner !== undefined) return { primitive: { primitive: PRIMITIVES.get(el.type), ...fields } };
  if (el.type === Circle) {
    const d = 2 * ((el.props as CircleProps).radius ?? 0.5);
    return { shape: "circle", size: [d, d] };
  }
  if (el.type === Rectangle || el.type === Ellipse) {
    const [hx, hy] = (el.props as RectangleProps).half_size ?? (el.type === Rectangle ? [0.5, 0.5] : [1, 0.5]);
    return { shape: el.type === Rectangle ? "rect" : "ellipse", size: [2 * hx, 2 * hy] };
  }
  const p = el.props as RegularPolygonProps;
  const d = 2 * (p.circumcircle?.radius ?? 0.5);
  return { shape: "polygon", size: [d, d], sides: p.sides ?? 6 };
}

/** Bevy's Mesh2d with its Transform, a primitive and a ColorMaterial: <Shape2D> in Bevy's 2D world. */
export function Mesh2d({ transform, visibility, children, ...rest }: Mesh2dProps) {
  const { width, height } = useVideoConfig();
  let shape: ShapeOf | undefined, material: ColorMaterialProps | undefined, shader: Record<string, unknown> | undefined;
  for (const c of Children.toArray(children)) {
    if (!isValidElement(c)) continue;
    if (c.type === ColorMaterial) material = c.props as ColorMaterialProps;
    else if (c.type === Material) shader = materialShader(c.props as MaterialProps);
    else if (PRIMITIVES.has(c.type)) {
      if (shape) throw new TypeError("Mesh2d: one primitive");
      shape = shapeOf(c);
    } else throw new TypeError("Mesh2d takes one of bevy_math's 2D primitives (<Circle>, <Capsule2d>, ...) and a <ColorMaterial> or <Material>");
  if (material && shader) throw new TypeError("Mesh2d: one material, a <ColorMaterial> or a <Material>");
  }
  if (!shape) throw new TypeError("Mesh2d: give it a primitive, e.g. <Circle radius={40} />");
  if (visibility === "Hidden") return null;
  const [tx, ty, tz] = transform?.translation ?? [0, 0, 0];
  const [sx, sy] = transform?.scale ?? [1, 1, 1];
  // Bevy turns counterclockwise about +Z; Shape2D's rotation is clockwise, as the page's
  const turn = transform?.rotation ? -eulerOf(transform.rotation)[2] : 0;
  // the material's colour as Shape2D's hex, and its alpha as Shape2D's opacity (raea-sys-sprite's ColorMaterial alpha)
  const c = material?.color ?? "#ffffff";
  const s = typeof c === "string" ? undefined : c instanceof LinearRgba ? c.to_srgba() : c;
  const color = s ? new Srgba(s.red, s.green, s.blue, 1).to_hex() : c as string;
  const opacity = (rest.opacity ?? 1) * (s ? s.alpha : 1);
  return <Shape2D {...rest} {...shape} color={color} {...(s ? { srgb: [s.red, s.green, s.blue] as [number, number, number] } : {})} {...(s || rest.opacity !== undefined ? { opacity } : {})} {...(material?.texture ? { texture: material.texture } : {})} {...(material?.alpha_mode ? { alpha_mode: material.alpha_mode } : {})} {...(material?.uv_transform ? { uv_transform: material.uv_transform } : {})} {...(shader ? { shader } : {})} x={width / 2 + tx} y={height / 2 - ty} z={tz} rotation={turn} scale={sx === sy ? sx : [sx, sy]} />;
}
