// @raea/motion: React components and hooks for time. This slice is the pure TypeScript and React part; drawing to
// pixels is the renderer's job (Bevy, through a Clock per frame).
// This entry runs inside the engine's V8, so it imports nothing beyond React; renderFrame is in ./test.
export { Easing, type EasingFn } from "./easing";
export { interpolate, type ExtrapolateMode, type InterpolateOptions } from "./interpolate";
export { spring, measureSpring, type SpringConfig, type SpringOptions } from "./spring";
export {
  Clock,
  useCurrentFrame,
  useVideoConfig,
  type ClockProps,
  type TimelineWord,
  type VideoConfig,
} from "./clock";
export {
  AbsoluteFill,
  Freeze,
  Loop,
  Sequence,
  Series,
  type AbsoluteFillProps,
  type FreezeProps,
  type LoopProps,
  type SequenceLayout,
  type SequenceProps,
  type SeriesSequenceProps,
} from "./timeline";
export {
  Audio,
  Img,
  MEDIA_ROOT,
  Video,
  staticFile,
  type AudioProps,
  type ImgProps,
  type MediaProps,
  type VideoProps,
  type VolumeProp,
} from "./media";
export { word, useWords, findPhrase, type WordOptions } from "./words";
export {
  Composition,
  getCompositions,
  getRoot,
  registerRoot,
  type CompositionProps,
  type RegisteredComposition,
} from "./registry";

export { Region, useRegion, proofBoxes, regionBand, type RegionName, type RegionBox, type RegionProps, type ProofInput, type ProofPart } from "./region";
export { lyria, sao, isGeneratedAsset, type AssetKind, type GeneratedAsset } from "./assets";
export { useInput, useGameState, useGameEvents, type Playtest } from "./games";
export { Overlay, overlays } from "./overlay";
export type { FrameInput, InputTimeline, Perception, Rect } from "./clock";
export { Mix, type MixProps } from "./mix";
