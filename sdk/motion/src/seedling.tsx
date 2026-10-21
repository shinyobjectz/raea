// @raea/motion/seedling: a game's samples played by bevy_seedling on raea's one mixer core (sdk/react/docs/audio-stack.md).
// While a <Seedling> is on the page its sample plays, from the frame it first shows, in seedling's graph stepped beside
// the game (raea-seedling): a player per element, with its effects (raea's Tone effects as seedling nodes) in its chain.
// Gone from the page, its player stops. A render and a play of the game hear the same samples.
//
//   {hit && <Seedling src={staticFile("hit.wav")} id={`hit-${hits}`} volume={0.8} />}
//   <Seedling src={staticFile("bed.wav")} loop fx={[{ kind: "Freeverb", options: { roomSize: 0.8, wet: 0.3 } }]} />
import type { ReactNode } from "react";
import { isGeneratedAsset, type GeneratedAsset } from "./assets";

export interface SeedlingFx {
  /** one of @raea/motion/tone's effect classes by name: Filter, Reverb, Freeverb, FeedbackDelay, Compressor, EQ3, ... */
  kind: string;
  /** its options, by Tone's names */
  options?: Record<string, unknown>;
}

export interface SeedlingProps {
  /** the sample: a media file (staticFile), or a generated sound (sao(...), tokensynth(...)) from the asset store */
  src: string | GeneratedAsset;
  /** the player's identity while it plays: a new id starts a new player (default: the src) */
  id?: string;
  /** linear gain, from the first frame (default 1) */
  volume?: number;
  /** playback speed (default 1); may change frame to frame */
  speed?: number;
  loop?: boolean;
  /** a serial chain of effects on this player */
  fx?: SeedlingFx[];
  /** where it sounds from in the 3D scene (metres, the Canvas3D's frame): heard from the scene's camera, panned, fading
   * and dulling with distance, pitched by its speed toward or away (doppler); may move frame to frame. Unset: a sound
   * with no place */
  at?: [number, number, number];
  /** a lowpass (Hz) for what stands between it and the camera: a wall, a window */
  muffle?: number;
}

export function Seedling(props: SeedlingProps): ReactNode {
  const gen = isGeneratedAsset(props.src);
  if (!gen && (typeof props.src !== "string" || !props.src)) throw new TypeError(`Seedling: src is a media file or a generated sound, got ${JSON.stringify(props.src)}`);
  const src = gen ? (() => { const u = props.src as GeneratedAsset; return { generate: u.kind, prompt: u.prompt, seconds: u.seconds, ...(u.options ?? {}) }; })() : props.src;
  const spec = {
    key: props.id ?? (gen ? `${(props.src as GeneratedAsset).kind}:${(props.src as GeneratedAsset).prompt}` : (props.src as string)),
    src,
    volume: props.volume ?? 1,
    speed: props.speed ?? 1,
    loop: !!props.loop,
    fx: (props.fx ?? []).map((f) => ({ kind: f.kind, options: f.options ?? {} })),
    ...(props.at ? { at: props.at } : {}),
    ...(props.muffle !== undefined ? { muffle: props.muffle } : {}),
  };
  return <audio data-seedling={JSON.stringify(spec)} />;
}

/** A sound in the 3D scene: a <Seedling> at a place (`at`), heard from the scene's camera as it moves, in a render and
 * in a game alike. A moving emitter (a car, a fly, a pan sliding) takes its place frame by frame.
 *
 *   <Sound3D src={staticFile("kettle.wav")} at={onRun(right, 2.7, 0.35, 1.1)} loop volume={0.6} /> */
export function Sound3D(props: SeedlingProps & { at: [number, number, number] }): ReactNode {
  const a = props.at;
  if (!Array.isArray(a) || a.length !== 3 || !a.every((x) => Number.isFinite(x))) throw new TypeError(`Sound3D: at is [x, y, z] in metres, got ${JSON.stringify(a)}`);
  return <Seedling {...props} />;
}
