// One clock (docs/stage-guards.feature SG5). A part's frame props are frames of the Sequence it plays in, as
// useCurrentFrame counts them. One set at or past that Sequence's last frame means the part never does what the prop
// says, and it is almost always a frame from another clock passed down as a prop: the stage film's store quote was
// given `at` 1013 in a 150-frame scene, so its reveal never started. Each part that takes such a frame refuses it here,
// at its first frame, naming the prop and the fix.
import { useVideoConfig } from "@raea/app/clock";

/** A frame prop: its name and its value (left out: nothing to check). */
export type FrameProp = readonly [prop: string, frame: number | undefined];

/** Refuses each frame prop at or past the last frame of a Sequence `durationInFrames` long. */
export function checkSequenceFrames(owner: string, name: string | undefined, frames: readonly FrameProp[], durationInFrames: number): void {
  if (!Number.isFinite(durationInFrames)) return;
  for (const [prop, v] of frames) {
    if (v === undefined || !(v >= durationInFrames)) continue;
    throw new RangeError(`${owner}${name ? ` '${name}'` : ""}: ${prop} ${v} is after this Sequence's last frame (${durationInFrames - 1}). Give ${prop} in this Sequence's frames (useCurrentFrame), never a frame passed down as a prop, or put the scene in a <Sequence from>.`);
  }
}

/** The same, for the Sequence the calling part is in. */
export function useSequenceFrames(owner: string, name: string | undefined, frames: readonly FrameProp[]): void {
  checkSequenceFrames(owner, name, frames, useVideoConfig().durationInFrames);
}

/** The frame an animation's first key falls on, from the animation's start: the frame it starts doing anything. A key
 *  map ({ 0: 0, 6: 1 }), a track ({ keys: [[0, 0], [6, 1]] }) or a list of tracks, by property, or a move with them. */
export function firstKey(spec: unknown): number | undefined {
  if (!spec || typeof spec !== "object") return undefined;
  const tracks = (spec as { tracks?: unknown; frames?: unknown }).tracks !== undefined && (spec as { frames?: unknown }).frames !== undefined ? (spec as { tracks: object }).tracks : spec;
  let first: number | undefined;
  const see = (f: number) => { if (Number.isFinite(f) && (first === undefined || f < first)) first = f; };
  const track = (t: unknown) => {
    if (!t || typeof t !== "object") return;
    if (Array.isArray((t as { keys?: unknown }).keys)) for (const k of (t as { keys: unknown[] }).keys) Array.isArray(k) && see(Number(k[0]));
    else for (const k of Object.keys(t)) see(Number(k));
  };
  for (const t of Object.values(tracks)) Array.isArray(t) ? t.forEach(track) : track(t);
  return first;
}
