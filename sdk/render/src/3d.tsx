// @raea/render/3d: 3D in the same tree (spec: "3D in the same tree"). Every component renders as an element that
// carries its name (data-3d) and props (data-props, JSON) for Bevy to draw; a Mesh also carries its Manifold
// expression (data-shape). Engine-safe: this module imports nothing beyond React.
import { toHex } from "./colour";
export { Canvas3D, Cuboid, Sphere, Plane3d, Cylinder, Torus, Cone, Capsule, Capsule3d, BufferMesh, Text3D, SurfaceShader, ThreeShaderMaterial, surfaceShader, samplerSpec, FramePass, Compute, PhaseItem, MeshVertexAttribute, ATTRIBUTE_POSITION, ATTRIBUTE_NORMAL, ATTRIBUTE_UV_0, ATTRIBUTE_UV_1, ATTRIBUTE_TANGENT, ATTRIBUTE_COLOR, Atmosphere, eulerOf, Entity, Refinish, clipEvents, Model, ImagePlane, surfaceTurn, Decal, Plate, Bloom, Fog, ToneMapping, Grade, Environment, EnvironmentMapLight, Skybox, AmbientOcclusion, AntiAliasing, Shadows, VolumetricFog, FogVolume, Fire, dofOf, DepthOfField, Outline, Fisheye, LensDistortion, MotionBlur, checkPass, Look, Scene, Entities, BevyComponent, Gizmo, ShaderMesh, BevyResource, Span, RichText, Instances, scatter, Grass } from "./3d/all";
export type { Vec3, Canvas3DProps, CuboidProps, SphereProps, CylinderProps, TorusProps, Plane3dProps, ConeProps, CapsuleProps, Capsule3dProps, BufferMeshProps, Text3DProps, Animate3d, SurfaceDrawProps, ToonBand, ToonRim, ToonHighlight, ToonOutline, SurfaceShaderSpec, ThreeShaderMaterialProps, ThreeUniform, SamplerSpec, SamplerWrap, FramePassProps, ComputeProps, MeshSlab, PhaseItemProps, PhaseAttribute, VertexFormat, MeshAttribute, AtmosphereProps, BevyParts, EntityProps, ModelProps, AnimGraphNode, ImagePlaneProps, DecalProps, PlateProps, BloomProps, FogProps, FogColor, FogFalloff, ToneMappingProps, GradeSection, GradeProps, EnvironmentProps, EnvironmentMapLightProps, SkyboxProps, AmbientOcclusionProps, AntiAliasingProps, ShadowsProps, VolumetricFogProps, FogVolumeProps, FireProps, DepthOfFieldProps, OutlineProps, LensCommon, FisheyeProps, LensDistortionProps, MotionBlurProps, LookProps, LookPass, SceneProps, BevyComponentProps, GizmoProps, ShaderMeshProps, SpanProps, GlyphInfo, GlyphStyle, RichTextProps, Instance, InstancesProps, PlanBox, ScatterOptions, GrassProps, GrassLod } from "./3d/all";

export { CrossSection, crossSection, manifold, outline, Solid, type Bounds, type Polygons, type Section } from "./solid";
// blended distance fields and generated growth (trees, lofts, terrain, boulders, rosettes), as Solids (grow.ts)
export { Field, sdf, grow, type TreeOptions, type LoftOutline } from "./grow";
// placement by relations, solved natively (place.ts)
export { place, type PlaceRule, type PlaceObject, type Placed, type Layout, type PlaceFinding } from "./place";

export { Volume, type VolumeProps, type VolumeSource, type VolumeCommon } from "./paint";

// footage with 3D standing in its world: the camera solved, the plate behind, the subject in front (matchmove.tsx)
// gaussian splats: a reconstructed scene or object, placed and animated by props (splat.tsx)
export { Splat, type SplatProps, type SplatWave, type SplatMorph } from "./splat";

// physics: rigid bodies baked by Rapier, glitches on parts, goo baked by MLS-MPM (physics.tsx)
// a live picture on a surface: a nested composition or game drawn as a CRT (screen.tsx)
export { Screen, type ScreenProps, type ScreenSource, type CrtSettings, type ModelScreens } from "./screen";

export { pick, pointerRay, type Hit, type PickCamera, type Pickable } from "./pick";

export { Matchmove, useMatchmove, matchmoveView, matchmoveIndex, checkMatchmove, type MatchmoveProps, type MatchmoveSolve, type MatchmoveState, type MatchmoveCamera } from "./matchmove";

export { toHex };
// 2D over the 3D (docs/sdk.feature S3c)
export { Sprite, Shape2D } from "./3d/all";
export type { Place2dProps, SpriteProps, Shape2DProps, AlphaMode2d } from "./3d/all";
export type { GizmoColor } from "./3d/all";
