// docs/sdk.feature S1b (M1): a Bevy-named entry point, re-exporting what today lives in the files named. Sources move
// into it at M5; until then the old subpaths and this one export the same values.
// raea/post_process (bevy_post_process, bevy_anti_alias): what's done to the frame after it's drawn.
export { Bloom, ToneMapping, Grade, DepthOfField, MotionBlur, Fisheye, LensDistortion, AntiAliasing, Outline, Look, FramePass, type FramePassProps, Compute, type ComputeProps, type BloomProps, type ToneMappingProps, type GradeProps, type GradeSection, type DepthOfFieldProps, type MotionBlurProps, type FisheyeProps, type LensDistortionProps, type LensCommon, type AntiAliasingProps, type OutlineProps, type LookProps } from "./3d";
// grade looks for shots (docs/make.feature MK17): clean, cinematic, gritty, vintage
export { GRADES, gradeLook, type GradeLook } from "@raea/render/3d/grades";
