// docs/sdk.feature S1b (M1): a Bevy-named entry point, re-exporting what today lives in the files named. Sources move
// into it at M5; until then the old subpaths and this one export the same values.
// raea/camera (bevy_camera): cameras and the views they make.
export { Canvas3D, type Canvas3DProps } from "./3d";
export { Matchmove, useMatchmove, matchmoveView, matchmoveIndex, checkMatchmove, type MatchmoveProps, type MatchmoveSolve, type MatchmoveState, type MatchmoveCamera } from "@raea/render/matchmove";
export { Camera3d, Hdr, type Camera3dProps, type CameraViewport, type ClearColorConfig, type Projection } from "@raea/render/bevy3d";
// a Bevy texture (a camera's target) drawn in an element (B3 H7)
export { TextureView } from "@raea/render/bevy3d";
// a camera path paced by time, so it can't lurch at a key (docs/stage-guards.feature SG7)
export { cameraPath, type CameraKey, type CameraPath, type CameraPose, type CameraVec3 } from "@raea/render/camera/path";
// a crew's moves, focus and a handheld walk, each a camera path (docs/make.feature MK14)
export { shotMove, focusAt, handheld, walkPath, WALK, lens, LENSES, STEP_HZ, type ShotMove, type MoveKind, type Focus, type HandheldProfile } from "@raea/render/camera/moves";
// page type that names a 3D part, placed through the shot's camera (docs/stage-guards.feature SG8)
export { Label, LABEL_MIN, type LabelProps, type LabelCamera } from "@raea/render/3d/label";
