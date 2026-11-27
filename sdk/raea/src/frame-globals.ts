// Browser globals a composition's runtime doesn't have, installed only where one is missing (#56): components written
// for a web page call them. requestAnimationFrame's callbacks run once per frame the composition draws, given that frame's
// time (f * 1000 / fps ms), so a frame drawn again (a resumed render, `--at`) gets the same calls; one asked for while a
// frame draws runs at the next frame. `new Image()` loads: setting its src fires load (and onload) on the next microtask,
// so a preload resolves, and by then naturalWidth and naturalHeight are the picture's size (#253): a data: URL's from its
// header, a file's or URL's from the host (native asset.load, which finds staticFile's /media/ paths as <Img> does).
// width and height are the ones given (the constructor's, or set) else the natural size, as a browser has them. A file
// the host can't find fires error (and onerror) instead. Under plain bun, with no host, a file's size stays 0.

import { hasNative, native } from "@raea/app/native";
import { sniffSize } from "@raea/ui/canvas";

type FrameCallback = (time: number) => void;

let queue = new Map<number, FrameCallback>();
let next = 1;
let ran = -1;
/** the frame being drawn's time, for a callback run while it's held */
let now = 0;
let pumping = false;

/** A callback asked for while the frame is held (delayRender) runs on a timer with that frame's time, as a browser's
 *  runs while a page waits: a hold released from requestAnimationFrame (two rAFs after a ref lands) would otherwise
 *  wait for a frame that can't start until it's released (layout-reads LR9). */
function pumpHeld(): void {
  // only a composition's own holds (delayRender's, named in `open`), not <R3F>'s while its scene loads
  if (pumping || !((globalThis as { __raeaHold?: { open?: Map<number, string> } }).__raeaHold?.open?.size! > 0)) return;
  pumping = true;
  setTimeout(() => {
    pumping = false;
    const due = queue;
    queue = new Map();
    for (const cb of due.values()) {
      try {
        cb(now);
      } catch (e) {
        console.error(e);
      }
    }
  }, 16);
}

/** Frame `f` at `fps` starts: the callbacks asked for before it run, once (a second draw of the same frame runs none). */
export function frameCallbacks(f: number, fps: number): void {
  const time = (f * 1000) / (fps > 0 ? fps : 30);
  now = time;
  if (f === ran || queue.size === 0) return;
  ran = f;
  const due = queue;
  queue = new Map();
  for (const cb of due.values()) {
    try {
      cb(time);
    } catch (e) {
      console.error(e);
    }
  }
}

type Listener = (e: { type: string; target: unknown; currentTarget: unknown }) => void;

/** A picture's size: a data: URL's from its header, anything else from the host (asset.load); null when it has none
 *  (no host, or a format the header doesn't say). Throws the host's error for a file that isn't there. */
function pictureSize(src: string): [number, number] | null {
  if (src.startsWith("data:")) return sniffSize(src);
  if (src.startsWith("blob:") || !hasNative()) return null;
  const got = native("asset.load", { path: src });
  return got && typeof got.width === "number" && typeof got.height === "number" ? [got.width, got.height] : null;
}

/** A preloadable image: its src set, load fires, with the picture's size. */
class FrameImage {
  onload: Listener | null = null;
  onerror: Listener | null = null;
  complete = true;
  naturalWidth = 0;
  naturalHeight = 0;
  alt = "";
  crossOrigin: string | null = null;
  decoding = "auto";
  loading = "eager";
  #src = "";
  #width: number | null;
  #height: number | null;
  #loads = 0;
  #listeners = new Map<string, Set<Listener>>();

  constructor(width?: number, height?: number) {
    this.#width = width ?? null;
    this.#height = height ?? null;
  }

  get width(): number {
    return this.#width ?? this.naturalWidth;
  }

  set width(v: number) {
    this.#width = Number(v) || 0;
  }

  get height(): number {
    return this.#height ?? this.naturalHeight;
  }

  set height(v: number) {
    this.#height = Number(v) || 0;
  }

  get src(): string {
    return this.#src;
  }

  set src(v: string) {
    const src = String(v);
    this.#src = src;
    this.complete = false;
    this.naturalWidth = 0;
    this.naturalHeight = 0;
    const load = ++this.#loads;
    queueMicrotask(() => {
      // a src set again before this one loaded: only the last one's load fires, as in a browser
      if (load !== this.#loads) return;
      let type = "load";
      try {
        const size = pictureSize(src);
        if (size) [this.naturalWidth, this.naturalHeight] = size;
      } catch (err) {
        type = "error";
        console.warn(`new Image(): ${src} didn't load: ${err instanceof Error ? err.message : String(err)}`);
      }
      this.complete = true;
      const e = { type, target: this, currentTarget: this };
      (type === "load" ? this.onload : this.onerror)?.call(this, e);
      for (const l of this.#listeners.get(type) ?? []) l.call(this, e);
    });
  }

  addEventListener(type: string, l: Listener): void {
    const set = this.#listeners.get(type) ?? new Set();
    set.add(l);
    this.#listeners.set(type, set);
  }

  removeEventListener(type: string, l: Listener): void {
    this.#listeners.get(type)?.delete(l);
  }

  decode(): Promise<void> {
    return Promise.resolve();
  }
}

const g = globalThis as any;
if (typeof g.requestAnimationFrame !== "function") {
  g.requestAnimationFrame = (cb: FrameCallback) => {
    const id = next++;
    queue.set(id, cb);
    pumpHeld();
    return id;
  };
  g.cancelAnimationFrame = (id: number) => void queue.delete(id);
}
if (typeof g.Image !== "function") g.Image = FrameImage;
