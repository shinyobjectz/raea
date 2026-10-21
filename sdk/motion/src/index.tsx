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
  song, songAt, useSong, SongPlot,
  type Song, type SongMap, type SongMoment, type SongEvent, type SongEventKind, type SongSection, type SongBar, type SongHit,
  type SongFeatures, type SongLevel, type SongOptions, type UseSongOptions, type CutOptions, type FrameRange, type SongPlotProps,
} from "./song";
export {
  Composition,
  getCompositions,
  getRoot,
  registerRoot,
  type CompositionProps,
  type RegisteredComposition,
} from "./registry";

export { Region, useRegion, proofBoxes, regionBand, type RegionName, type RegionBox, type RegionProps, type ProofInput, type ProofPart } from "./region";
export { lyria, sao, fish, isGeneratedAsset, type AssetKind, type GeneratedAsset } from "./assets";
export { useInput, useGameState, useGameEvents, stepAll, keyEdges, type Playtest } from "./games";
export { actionsIn, applyActions, spaceIn, voidIn, ACTION_KEY, SPACE_KEY, VOID_KEY, type PanelAction, type PanelActions } from "./panel";
export {
  machine, firstDecider, replayDecider, stateOf, inState, transitionOf,
  type Machine, type MachineConfig, type StateConfig, type TransitionConfig, type Transitions, type Guard, type Action,
  type Actions, type Snapshot, type Watched, type Edge, type Analysis, type AnyMachine, type StateName, type Decider,
  type Decision, type ChoicePoint, type TransitionRecord, type StepOptions,
} from "./machine";
export { Overlay, overlays } from "./overlay";
export type { FrameInput, InputTimeline, Perception, Rect } from "./clock";
export { Mix, type MixProps } from "./mix";
export {
  Animate, Stagger, propsFingerprint, track, move, delay, reverse, curve, curveFn, timeline, sequence, sequenceGap, parallel, stagger, gridDelay, sampleAnimation,
  rise, pop, slam, slide, wipe, fill, tick, strike, pushIn, fade, moves, toMove,
  type AnimateProps, type AnimateOwnProps, type StaggerProps, type TrackSpec, type Move, type MoveLike, type MoveName, type Keys, type Curve, type CurveName,
  type Plan, type Step, type Group, type GridFrom, type Animation, type AnimTrack, type AnimKey,
} from "./animate";
