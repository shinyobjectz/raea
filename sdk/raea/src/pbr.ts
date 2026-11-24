// docs/sdk.feature S1b (M1): a Bevy-named entry point, re-exporting what today lives in the files named. Sources move
// into it at M5; until then the old subpaths and this one export the same values.
// raea/pbr (bevy_pbr, bevy_material): materials, fog, volumes and the paint look's things.
export { SurfaceShader, ThreeShaderMaterial, type ThreeShaderMaterialProps, Refinish, Fog, VolumetricFog, FogVolume, Fire, Decal, ShaderMesh, AmbientOcclusion, type SurfaceShaderSpec, type SurfaceDrawProps, type ToonBand, type ToonRim, type ToonHighlight, type ToonOutline, type FogProps, type VolumetricFogProps, type FogVolumeProps, type FireProps, type DecalProps, type ShaderMeshProps, type AmbientOcclusionProps } from "./3d";
export * from "@raea/render/splat";
export * from "@raea/render/paint";
export { StandardMaterial, ExtendedMaterial, ToonMaterial, type ToonMaterialProps, GridExtension, PaintExtension, WearExtension, ProceduralExtension, ToonExtension, ShaderExtension, type StandardMaterialProps, type StandardMaterialAnimate, type ToonExtensionAnimate, type ExtendedMaterialProps, type GridExtensionProps, type PaintExtensionProps, type WearExtensionProps, type ProceduralExtensionProps, type ToonExtensionProps, type ShaderExtensionProps, type AlphaMode, type SampledImage, type MetallicRoughnessMaps } from "@raea/render/bevy3d";
// FogFalloff (bevy_pbr::fog): Bevy's falloff constructors, for a <Fog> inside the <Camera3d>
export { FogFalloff } from "@raea/render/bevy3d";
// Lightmap (bevy_pbr::lightmap): a bake on a <Model>'s glTF meshes, by the mesh's name
export { Lightmap, type LightmapProps } from "@raea/render/bevy3d";
// ShaderConstants: a `const` in one of the engine's own shaders (raea's, Bevy's, Solari's) set when the scene loads
export { ShaderConstants, type ShaderConstantsProps, type ShaderConstantValue } from "@raea/render/bevy3d";
