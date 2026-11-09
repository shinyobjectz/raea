// Bevy's own names for 3D (docs/sdk.feature S2a): Transform, Quat, Mesh3d, Camera3d with Projection, and
// StandardMaterial, and the lights (PointLight, SpotLight, DirectionalLight, AmbientLight, RectLight), each taking its Bevy fields as Bevy reflects them (snake_case, radians, quaternions, Bevy's
// defaults, Bevy's units) and drawn by the same components as 3d.tsx's three.js-shaped names, so the two vocabularies render alike.
//
//   <Camera3d transform={Transform.fromXyz(0, 2, 7).lookingAt([0, 0.5, 0])} />
//   <Mesh3d transform={Transform.fromXyz(0, 0.5, 0)}>
//     <Cuboid half_size={[0.5, 0.5, 0.5]} />
//     <StandardMaterial base_color="#c84" perceptual_roughness={0.4} />
//   </Mesh3d>
import { eulerOf } from "./3d";
export { Quat, Affine2, Transform, Mesh3d, Camera3d, Srgba, LinearRgba, StandardMaterial, GridExtension, PaintExtension, WearExtension, ProceduralExtension, ToonExtension, ShaderExtension, ExtendedMaterial, ToonMaterial, Mesh2d, Circle, Rectangle, Ellipse, RegularPolygon, CircularSector, CircularSegment, Annulus, Capsule2d, Rhombus, Triangle2d, Segment2d, Polyline2d, ConvexPolygon, ColorMaterial, PointLight, SpotLight, DirectionalLight, AmbientLight, GlobalAmbientLight, RectLight, light_consts } from "./bevy3d/all";
export type { EulerRot, TransformValue, TransformBuilt, Mesh3dProps, Projection, Camera3dProps, CameraViewport, ClearColorConfig, AlphaMode, StandardMaterialProps, StandardMaterialAnimate, ToonExtensionAnimate, SampledImage, MetallicRoughnessMaps, GridExtensionProps, PaintExtensionProps, WearExtensionProps, ProceduralExtensionProps, ToonExtensionProps, ShaderExtensionProps, ExtendedMaterialProps, ToonMaterialProps, Mesh2dProps, CircleProps, RectangleProps, EllipseProps, RegularPolygonProps, RingProps, ArcProps, AnnulusProps, Capsule2dProps, RhombusProps, VerticesProps, ColorMaterialProps, MeshAttributes, PointLightProps, SpotLightProps, DirectionalLightProps, AmbientLightProps, RectLightProps } from "./bevy3d/all";

export { eulerOf };
// ray-traced lighting: Bevy's Solari (docs/raytracing.feature RT3)
export { SolariLighting, Pathtracer, type SolariLightingProps, type PathtracerProps } from "./bevy3d/raytracing";
export { ConicalFrustum, type ConicalFrustumProps } from "./3d/shapes";
// a Bevy texture (a camera's target) drawn in an element (B3 H7)
export { TextureView } from "./layers";
// your own Material (docs/control.feature FC2a)
export { Material, type MaterialProps } from "./bevy3d/all";
// bevy_color's other spaces (docs/control.feature FC1f)
export { Hsla, Okhsla, Oklaba, Oklcha, Xyza } from "./bevy3d/all";
// FogFalloff, Bevy's constructors, for a <Fog> inside the <Camera3d> (docs/control.feature FC0a)
export { FogFalloff } from "./bevy3d/fog";
// Bevy's Hdr camera marker, inside the <Camera3d> (docs/control.feature FC0a)
export { Hdr } from "./bevy3d/core";
// Lightmap (bevy_pbr::lightmap): a bake on a <Model>'s glTF meshes by name (docs/control.feature FC0a)
export { Lightmap, type LightmapProps } from "./bevy3d/lightmap";
// bevy_ui's Node, Text and TextSpan, and their values, drawn by the page
export { Node, GhostNode, Text, TextSpan, TextShadow, Text2d, UiScale, Viewport, UiTargetCamera, FontVariationTag, type NodeProps, type TextProps, type TextStyleProps, type TextRunProps, type Text2dProps } from "./bevy3d/ui";
export { UiTexture, targetImage } from "./bevy3d/ui-texture";
export { EditableText, type EditableTextProps, type TextCursorStyle } from "./bevy3d/ui-edit";
export { ImageNode, InlineImage, ImagePlugin, TextureAtlasLayout, type TextureSlicer, type SliceScaleMode, type ImageNodeProps } from "./bevy3d/ui-image";
export { px, percent, vw, vh, vmin, vmax, auto, UiRect, BorderRadius, CornerRadius, Overflow, OverflowClipMargin, type Val } from "./bevy3d/ui-val";
export { LinearGradient, RadialGradient, ConicGradient, ColorStop, AngularColorStop, UiPosition, type Gradient, type RadialGradientShape, type InterpolationColorSpace } from "./bevy3d/ui-gradient";
export { GridTrack, RepeatedGridTrack, GridPlacement, type GridTrackRepetition, type GridAutoFlow, type MinTrackSizingFunction, type MaxTrackSizingFunction } from "./bevy3d/ui-grid";
export { Rot2, Val2, UiTransform, BoxShadow, BoxShadowSamples, type ShadowStyle } from "./bevy3d/ui-transform";
export { MaterialNode, type MaterialNodeProps } from "./bevy3d/ui-material";
export type { Popover, PopoverPlacement } from "./bevy3d/ui-pick";
export { FontFeatureTag, FontWidth } from "./bevy3d/ui-features";
export { GlobalUiDebugOptions, type UiDebugOptions } from "./bevy3d/ui-debug";
export type { IgnoreScroll } from "./bevy3d/ui-scroll";
export { Scrollbar, ScrollbarThumb, type ScrollbarProps, type ScrollbarThumbProps, type ControlOrientation } from "./bevy3d/ui-scrollbar";
export { FontAtlasSet, type FontAtlas, FontCx, FontSource, type GenericFontFamily } from "./bevy3d/ui-font";
