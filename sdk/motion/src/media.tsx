// Video, Audio, Img and staticFile (spec: "Media plays in step with the frame"). These render as plain elements
// that carry their props for the renderer, with the timing already worked out for the frame being rendered.
import type { CSSProperties, ImgHTMLAttributes } from "react";
import { isGeneratedAsset, type GeneratedAsset } from "./assets";
import { useClockState, useLocalTime } from "./clock";
import { jsx } from "react/jsx-runtime";

export type VolumeProp = number | ((mediaFrame: number) => number);

export interface MediaProps {
  src: string;
  startFrom?: number;
  endAt?: number;
  trimBefore?: number;
  trimAfter?: number;
  /** linear gain, 1 being unity (0.5 is about -6 dB, 1.78 about +5 dB), or a function of the media's own frame */
  volume?: VolumeProp;
  /** ours: the gain in decibels (or a function of the media's own frame returning it); wins over volume */
  gainDb?: VolumeProp;
  /** ours: seconds to ramp the sound in from silence at its start (applied by the renderer at the audio's rate) */
  fadeIn?: number;
  /** ours: seconds to ramp the sound out to silence at its end */
  fadeOut?: number;
  playbackRate?: number;
  muted?: boolean;
  loop?: boolean;
  /** the bus it plays into: a named channel of the edit's Tone graph (music, sfx, voice, ambience, ui), so it takes
   *  that bus's volume, effects and ducking (@raea/motion/tone `buses`) */
  bus?: string;
  name?: string;
  className?: string;
  style?: CSSProperties;
}

export interface VideoProps extends MediaProps {
  /** ours: frame the take on the largest face */
  framing?: "face";
  /** ours: multiplies the framing's scale (1.15 is the usual) */
  punchIn?: number;
}

export type AudioProps = Omit<MediaProps, "style" | "className" | "src"> & {
  /** a file URL, or a generated asset from lyria() or sao() */
  src: string | GeneratedAsset;
};

interface MediaTiming {
  /** source frame shown or heard, or null when the media has ended and is silent/held */
  sourceFrame: number;
  ended: boolean;
  volume: number;
}

function pick(owner: string, a: number | undefined, aName: string, b: number | undefined, bName: string) {
  if (a !== undefined && b !== undefined) {
    throw new Error(`${owner}: ${aName} and ${bName} name the same thing; give only one of them`);
  }
  return a ?? b;
}

function timing(owner: string, props: MediaProps, frame: number, src: string): MediaTiming {
  const startFrom = pick(owner, props.startFrom, "startFrom", props.trimBefore, "trimBefore") ?? 0;
  const endAt = pick(owner, props.endAt, "endAt", props.trimAfter, "trimAfter");
  const rate = props.playbackRate ?? 1;
  if (!Number.isFinite(startFrom) || startFrom < 0) {
    throw new RangeError(`${owner} ${src}: startFrom must be a non-negative number, got ${startFrom}`);
  }
  if (endAt !== undefined && !(endAt > startFrom)) {
    throw new RangeError(`${owner} ${src}: endAt (${endAt}) must come after startFrom (${startFrom})`);
  }
  if (!(rate > 0) || !Number.isFinite(rate)) {
    throw new RangeError(`${owner} ${src}: playbackRate must be a positive number, got ${rate}`);
  }

  let sourceFrame = startFrom + Math.max(0, frame) * rate;
  let ended = false;
  if (endAt !== undefined && sourceFrame >= endAt) {
    if (props.loop) {
      const span = endAt - startFrom;
      sourceFrame = startFrom + ((sourceFrame - startFrom) % span);
    } else {
      // endAt is exclusive: hold the last frame before it
      sourceFrame = Math.max(startFrom, endAt - 1);
      ended = true;
    }
  }

  let volume: number;
  if (props.gainDb !== undefined) {
    const db = typeof props.gainDb === "function" ? props.gainDb(frame) : props.gainDb;
    if (typeof db !== "number" || Number.isNaN(db) || db === Infinity) {
      throw new RangeError(`${owner} ${src}: gainDb must be a number of decibels, got ${db}`);
    }
    volume = Math.pow(10, db / 20); // -Infinity dB is silence
  } else {
    const v = props.volume ?? 1;
    volume = typeof v === "function" ? v(frame) : v;
    if (typeof volume !== "number" || !(volume >= 0) || !Number.isFinite(volume)) {
      throw new RangeError(`${owner} ${src}: volume must be a non-negative linear gain, got ${volume}`);
    }
  }
  // (one object, read at once by the one caller: each call made one, for every playing sound every frame)
  TIMING.sourceFrame = sourceFrame;
  TIMING.ended = ended;
  TIMING.volume = volume;
  return TIMING;
}
const TIMING: MediaTiming = { sourceFrame: 0, ended: false, volume: 1 };

/** A fade length in seconds, checked; the renderer applies it at the audio's own rate, so it can be under a frame. */
function fadeSeconds(owner: string, props: MediaProps, name: "fadeIn" | "fadeOut", src: string): number {
  const v = props[name] ?? 0;
  if (typeof v !== "number" || !(v >= 0) || !Number.isFinite(v)) {
    throw new RangeError(`${owner} ${src}: ${name} must be a non-negative number of seconds, got ${v}`);
  }
  return v;
}

// `src`: the file the element plays (an <Audio>'s generated sound passes its file; its props were copied to give it)
function useMediaAttrs(owner: string, props: MediaProps, src = props.src as string) {
  const { fps } = useClockState(owner).config;
  const { frame, origin } = useLocalTime(owner);
  if (typeof src !== "string" || src === "") throw new Error(`${owner}: src is required`);
  const t = timing(owner, props, frame, src);
  const attrs: Record<string, string | number | undefined> = {
    src,
    "data-source-time": t.sourceFrame / fps,
    "data-source-frame": t.sourceFrame,
    "data-volume": props.muted ? 0 : t.volume,
    "data-fade-in": fadeSeconds(owner, props, "fadeIn", src),
    "data-fade-out": fadeSeconds(owner, props, "fadeOut", src),
    "data-playback-rate": props.playbackRate ?? 1,
    "data-start-from": props.startFrom ?? props.trimBefore ?? 0,
    "data-end-at": props.endAt ?? props.trimAfter,
    "data-loop": props.loop ? "true" : undefined,
    "data-muted": props.muted ? "true" : undefined,
    "data-ended": t.ended ? "true" : undefined,
    "data-name": props.name,
    // the bus it plays into (a named channel of the edit's Tone graph: music, sfx, voice, ambience, ui; @raea/motion/tone)
    "data-bus": props.bus,
    // the composition frame this clip's frame 0 falls on (its Sequence's start): a live player tells one sound from
    // another by it, however many come and go around it
    "data-from": origin,
  };
  return attrs;
}

/** A video clip in step with the frame; `data-source-time` is the source time (seconds) to show. */
export function Video(props: VideoProps) {
  const attrs = useMediaAttrs("Video", props);
  if (props.punchIn !== undefined && !(props.punchIn > 0)) {
    throw new RangeError(`Video ${props.src}: punchIn must be a positive number, got ${props.punchIn}`);
  }
  if (props.framing !== undefined && props.framing !== "face") {
    throw new RangeError(`Video ${props.src}: framing must be "face", got "${props.framing}"`);
  }
  const style: CSSProperties = { objectFit: "cover", objectPosition: "50% 50%", ...props.style };
  return (
    <video
      {...attrs}
      className={props.className}
      style={style}
      data-framing={props.framing}
      data-punch-in={props.punchIn}
    />
  );
}

/** Sound in step with the frame; no picture. A muted or ended Audio is silent. */
export function Audio(props: AudioProps) {
  const asset = isGeneratedAsset(props.src) ? props.src : null;
  // a generated sound is as long as it was made (its seconds): once its source time is past that it has ended, and it
  // leaves the page. A game's cues are Sequences without an end, so every sound it ever played stayed on the page with
  // its source time moving: a finished pong rendered 115 of them a frame and sent their 115 updates (12 KB)
  if (asset && !props.loop && props.endAt === undefined && props.trimAfter === undefined) {
    const { fps } = useClockState("Audio").config;
    const { frame } = useLocalTime("Audio");
    const startFrom = props.startFrom ?? props.trimBefore ?? 0;
    const rate = props.playbackRate ?? 1;
    if (rate > 0 && (startFrom + Math.max(0, frame) * rate) / fps >= asset.seconds) return null;
  }
  const attrs = useMediaAttrs("Audio", props, asset ? asset.src : (props.src as string));
  if (attrs["data-ended"]) attrs["data-volume"] = 0;
  if (asset) {
    attrs["data-generate"] = asset.kind;
    attrs["data-prompt"] = asset.prompt;
    attrs["data-seconds"] = asset.seconds;
  }
  // the attributes are the element's props as they are (a spread copied all of them, every frame)
  return jsx("audio", attrs);
}

export type ImgProps = ImgHTMLAttributes<HTMLImageElement> & { src: string };

/** An image; the renderer holds the frame until it has loaded and fails the render (naming src) if it can't. */
export function Img(props: ImgProps) {
  if (typeof props.src !== "string" || props.src === "") throw new Error("Img: src is required");
  // A low fetch priority keeps React's server renderer from hoisting a <link rel="preload"> for the image, which the
  // engine's renderer doesn't want; it loads every image before drawing the frame regardless.
  return <img fetchPriority="low" {...props} />;
}

/** Where the space's media folder is served from. */
export const MEDIA_ROOT = "/media/";

/** The URL of a file in the space's media folder, each path segment URL-encoded. */
export function staticFile(path: string): string {
  if (typeof path !== "string" || path.trim() === "") throw new Error("staticFile: give a path inside the media folder");
  if (/^[a-z][a-z0-9+.-]*:/i.test(path)) {
    throw new Error(`staticFile: "${path}" is a URL, not a path inside the media folder`);
  }
  const kept: string[] = [];
  for (const segment of path.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") {
      if (kept.length === 0) throw new Error(`staticFile: "${path}" leaves the media folder`);
      kept.pop();
      continue;
    }
    kept.push(segment);
  }
  if (kept.length === 0) throw new Error(`staticFile: "${path}" names the media folder itself, not a file in it`);
  return MEDIA_ROOT + kept.map(encodeURIComponent).join("/");
}
