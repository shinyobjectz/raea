// on* on a 3D part (docs/sdk.feature S3a: 2D and 3D share picking events): a <Mesh> (or Mesh3d) with onClick,
// onPointerDown and the rest has them called as a page element's are, from the frame's input and the part's place, as
// pick.ts picks: the camera and the parts' bounds alone, so a render and a play of the same input call the same
// handlers on the same frames.
//
//   <Mesh position={[0, 0.5, 0]} onClick={() => setOpen(true)} onPointerOver={() => setHot(true)}>
//     <Box args={[1, 1, 1]} />
//   </Mesh>
//
// Each frame, every part with a handler puts its bounds in (a box, or a sphere for a sphere scaled evenly) as the
// frame renders, the camera its view; once the frame commits, the pointer is picked against them once and each part's
// handlers for that frame are called. Frames go in order (a play, a render of a run of frames), as a page's own
// events do: a render of one frame on its own has no frame before it to press in.
import { createContext, useContext, useEffect, useRef } from "react";
import { useCurrentFrame, useVideoConfig } from "@raea/app/clock";
import { useInput } from "@raea/app/games";
import { hasNative, native } from "@raea/app/native";
import { pick, type Hit, type PickCamera, type Pickable } from "./pick";
import type { Vec3 } from "./3d";

/** A part's pointer event, as a DOM PointerEvent has it: the pointer in frame px, the hit in world space, and for
 * onWheel the wheel's turn. */
export interface Event3d {
  type: string;
  x: number;
  y: number;
  /** where the pointer's ray met the part (none for onPointerOut / onPointerLeave) */
  point?: Vec3;
  /** 0, the primary button */
  button: number;
  deltaX?: number;
  deltaY?: number;
  /** for onDragEnter, onDragOver, onDragLeave and onDrop: the part being dragged */
  dragged?: string;
}

type Handler = (e: Event3d) => void;

/** Each handler prop, by the event it's called for. */
export const POINTER_PROPS: Record<string, string> = {
  pointerdown: "onPointerDown",
  pointerup: "onPointerUp",
  click: "onClick",
  pointerover: "onPointerOver",
  pointerout: "onPointerOut",
  pointerenter: "onPointerEnter",
  pointerleave: "onPointerLeave",
  pointermove: "onPointerMove",
  wheel: "onWheel",
  // drag and drop, as Bevy picking's DragStart, Drag, DragEnd (on the part pressed) and DragEnter, DragOver,
  // DragLeave, DragDrop (on the part under it), by the web's names for the same
  dragstart: "onDragStart",
  drag: "onDrag",
  dragend: "onDragEnd",
  dragenter: "onDragEnter",
  dragover: "onDragOver",
  dragleave: "onDragLeave",
  drop: "onDrop",
};

/** A 4x4 matrix, column-major (glam's order). */
export type Mat4 = number[];

export const IDENTITY: Mat4 = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

const mul = (a: Mat4, b: Mat4): Mat4 => {
  const o = new Array(16).fill(0);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
  return o;
};

/** A part's matrix in its parent's: translate, then rotate as the engine does (Quat::from_euler(EulerRot::YXZ, y, x,
 * z): Ry Rx Rz), then scale. */
export function local(position: Vec3, rotation: Vec3, scale: number | Vec3, q?: [number, number, number, number]): Mat4 {
  const [sx, sy, sz] = typeof scale === "number" ? [scale, scale, scale] : scale;
  if (q) {
    const [x, y, z, w] = q;
    const r: Mat4 = [1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y), 0, 2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x), 0, 2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y), 0, 0, 0, 0, 1];
    const m = mul(r, [sx, 0, 0, 0, 0, sy, 0, 0, 0, 0, sz, 0, 0, 0, 0, 1]);
    m[12] = position[0]; m[13] = position[1]; m[14] = position[2];
    return m;
  }
  const [cx, sxr] = [Math.cos(rotation[0]), Math.sin(rotation[0])];
  const [cy, syr] = [Math.cos(rotation[1]), Math.sin(rotation[1])];
  const [cz, szr] = [Math.cos(rotation[2]), Math.sin(rotation[2])];
  const ry: Mat4 = [cy, 0, -syr, 0, 0, 1, 0, 0, syr, 0, cy, 0, 0, 0, 0, 1];
  const rx: Mat4 = [1, 0, 0, 0, 0, cx, sxr, 0, 0, -sxr, cx, 0, 0, 0, 0, 1];
  const rz: Mat4 = [cz, szr, 0, 0, -szr, cz, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
  // Euler XYZ, as the engine composes a part's rotation (runtime/scene `local`)
  const r = mul(mul(rx, ry), rz);
  const s: Mat4 = [sx, 0, 0, 0, 0, sy, 0, 0, 0, 0, sz, 0, 0, 0, 0, 1];
  const m = mul(r, s);
  m[12] = position[0];
  m[13] = position[1];
  m[14] = position[2];
  return m;
}
/** The world matrix of the part being rendered: a Group's, an Entity's or a Mesh's, for the parts inside. */
export const Place3d = createContext<Mat4>(IDENTITY);

/** `m` under its parent's world matrix. */
export function under(parent: Mat4, m: Mat4): Mat4 {
  return mul(parent, m);
}

const apply = (m: Mat4, p: Vec3): Vec3 => [
  m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
  m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
  m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
];

/** A shape's bounds in its own space: a box (its min and max) or a sphere's radius about its origin. */
export type LocalBounds = { box: [Vec3, Vec3] } | { radius: number };

/** The part's bounds in the world: a sphere stays one under an even scale, anything else is its box's corners' box. */
export function worldBounds(name: string, m: Mat4, b: LocalBounds): Pickable {
  if ("radius" in b) {
    const k = [Math.hypot(m[0], m[1], m[2]), Math.hypot(m[4], m[5], m[6]), Math.hypot(m[8], m[9], m[10])];
    if (Math.max(...k) - Math.min(...k) < 1e-9) return { name, sphere: { center: [m[12], m[13], m[14]], radius: b.radius * k[0] } };
    b = { box: [[-b.radius, -b.radius, -b.radius], [b.radius, b.radius, b.radius]] };
  }
  const [lo, hi] = b.box;
  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < 8; i++) {
    const p = apply(m, [i & 1 ? hi[0] : lo[0], i & 2 ? hi[1] : lo[1], i & 4 ? hi[2] : lo[2]]);
    for (let k = 0; k < 3; k++) {
      min[k] = Math.min(min[k], p[k]);
      max[k] = Math.max(max[k], p[k]);
    }
  }
  return { name, box: { min, max } };
}

/** A 2D part's place over the 3D (<Sprite>, <Shape2D>): its centre in frame px, its size, turned clockwise by
 * `rotation` radians; `round` for a circle or an ellipse; higher `z` on top. */
export interface Flat {
  x: number;
  y: number;
  w: number;
  h: number;
  rotation: number;
  z: number;
  round: boolean;
}

/** Whether `p` (frame px) is on the 2D part `f`. */
export function onFlat(p: { x: number; y: number }, f: Flat): boolean {
  const dx = p.x - f.x, dy = p.y - f.y;
  const c = Math.cos(f.rotation), s = Math.sin(f.rotation);
  // into the part's own axes: undo its clockwise turn (y is down)
  const u = (dx * c + dy * s) / (f.w / 2), v = (-dx * s + dy * c) / (f.h / 2);
  return f.round ? u * u + v * v <= 1 : Math.abs(u) <= 1 && Math.abs(v) <= 1;
}

interface Part {
  target?: Pickable;
  flat?: Flat;
  handlers: Record<string, Handler>;
}

/** What the last frame left: the part under the pointer, the one pressed, the pointer, and while a drag is on, the
 * part dragged and the one it is over. */
export interface Last {
  hover: string | null;
  down: string | null;
  drag?: string | null;
  over?: string | null;
  pointer?: { x: number; y: number; down: boolean };
}

/** A frame's events, by part: the pointer at `pointer` over `hover` (the part picked, or none) after `last`. */
export function events3d(last: Last, pointer: { x: number; y: number; down: boolean } | undefined, hover: string | null, point: Vec3 | undefined, wheel?: { x: number; y: number }): { events: Map<string, Event3d[]>; last: Last } {
  const events = new Map<string, Event3d[]>();
  const at = pointer ?? last.pointer ?? { x: 0, y: 0, down: false };
  const push = (id: string, type: string, extra: Partial<Event3d> = {}) => {
    const list = events.get(id) ?? [];
    list.push({ type, x: at.x, y: at.y, button: 0, ...(id === hover && point ? { point } : {}), ...extra });
    events.set(id, list);
  };
  if (last.hover !== hover) {
    if (last.hover !== null) {
      push(last.hover, "pointerout");
      push(last.hover, "pointerleave");
    }
    if (hover !== null) {
      push(hover, "pointerover");
      push(hover, "pointerenter");
    }
  } else if (hover !== null && pointer && last.pointer && (pointer.x !== last.pointer.x || pointer.y !== last.pointer.y)) {
    push(hover, "pointermove");
  }
  let down = last.down;
  let drag = last.drag ?? null;
  let over = last.over ?? null;
  const was = !!last.pointer?.down;
  const is = !!pointer?.down;
  const moved = !!pointer && !!last.pointer && (pointer.x !== last.pointer.x || pointer.y !== last.pointer.y);
  if (!was && is) {
    down = hover;
    if (hover !== null) push(hover, "pointerdown");
  } else if (was && is && down !== null && moved) {
    // a press that moves is a drag of the part pressed (Bevy's DragStart on the first move, then Drag each move);
    // the part under it, the dragged one aside, is entered, gone over and left
    if (drag === null) {
      drag = down;
      push(drag, "dragstart");
    }
    push(drag, "drag");
    const now = hover !== drag ? hover : null;
    if (over !== now && over !== null) push(over, "dragleave", { dragged: drag });
    if (now !== null) push(now, over === now ? "dragover" : "dragenter", { dragged: drag });
    over = now;
  } else if (was && !is) {
    if (hover !== null) push(hover, "pointerup");
    if (hover !== null && hover === last.down) push(hover, "click");
    // the drop, then the drag's end, then the part dropped on is left, in Bevy's order
    if (drag !== null) {
      if (over !== null) push(over, "drop", { dragged: drag });
      push(drag, "dragend");
      if (over !== null) push(over, "dragleave", { dragged: drag });
    }
    down = null;
    drag = null;
    over = null;
  }
  if (wheel && hover !== null && (wheel.x !== 0 || wheel.y !== 0)) push(hover, "wheel", { deltaX: wheel.x, deltaY: wheel.y });
  return { events, last: { hover, down, drag, over, pointer } };
}

// the frame's parts and camera as they render, and its events once worked out
const scene = {
  frame: Number.NaN,
  camera: undefined as PickCamera | undefined,
  parts: new Map<string, Part>(),
  done: Number.NaN,
  events: new Map<string, Event3d[]>(),
  last: { hover: null, down: null } as Last,
};

/** What the frame last rendered can be picked: each part with handlers, by id, and its bounds in the world. */
export function pickable3d(): ReadonlyMap<string, Pickable | undefined> {
  return new Map([...scene.parts].map(([id, p]) => [id, p.target]));
}

function begin(frame: number) {
  if (scene.frame === frame) return;
  // a frame that doesn't follow the last one starts with nothing pressed or under the pointer
  if (!(frame === scene.frame + 1)) scene.last = { hover: null, down: null };
  scene.frame = frame;
  scene.camera = undefined;
  scene.parts.clear();
}

/** The camera parts are picked through this frame (a PerspectiveCamera's or OrthographicCamera's props). */
export function useCamera3d(camera: PickCamera, active = true) {
  begin(useCurrentFrame());
  // a camera not drawing this frame (<Camera3d is_active={false}>) isn't the one the pointer picks through
  if (active) scene.camera = camera;
}

/** A part's handlers (its on* props) called for the frame's pointer events on it; `bounds` gives its shape's, in its
 * own space, under the world matrix `m`. Nothing without a handler, and then `bounds` isn't asked: a part with none
 * worked out its bounds every render, half of bloom_3d's React step (docs/control.feature FC1b). */
export function usePointer3d(id: string, m: Mat4, bounds: () => LocalBounds | undefined, props: Record<string, unknown>) {
  usePart(id, (handlers) => {
    const b = bounds();
    return b === undefined ? undefined : { target: worldBounds(id, m, b), handlers };
  }, props);
}

/** A 2D part's handlers, as usePointer3d's: picked in frame px over the 3D, before any 3D part. */
export function usePointer2d(id: string, flat: Flat, props: Record<string, unknown>) {
  usePart(id, (handlers) => ({ flat, handlers }), props);
}

const POINTER_ENTRIES = Object.entries(POINTER_PROPS);

function usePart(id: string, part: (handlers: Record<string, Handler>) => Part | undefined, props: Record<string, unknown>) {
  const handlers: Record<string, Handler> = {};
  let n = 0;
  for (const [type, prop] of POINTER_ENTRIES) if (typeof props[prop] === "function") (handlers[type] = props[prop] as Handler), n++;
  const made = n > 0 ? part(handlers) : undefined;
  const any = made !== undefined;
  // only a part with handlers follows the clock: reading it re-renders the part every frame, and every part reading
  // it made 10 000 still cubes re-render every frame (docs/control.feature FC1b). The three are context reads only,
  // which hold no hook slot in React (readContext) or in the live runtime (lite's useContext logs the read), so a
  // part whose handlers come and go keeps its hook order
  const frame = any ? useCurrentFrame() : -1;
  const input = any ? useInput() : undefined;
  const { width, height } = any ? useVideoConfig() : { width: 0, height: 0 };
  if (any) {
    begin(frame);
    scene.parts.set(id, made);
  }
  useEffect(() => {
    if (!any || !input) return;
    if (scene.done !== frame) {
      scene.done = frame;
      const p = input.pointer;
      const r = events3d(scene.last, p, ...pickFrame(p, { width, height }), input.wheel);
      scene.events = r.events;
      scene.last = r.last;
    }
    for (const e of scene.events.get(id) ?? []) handlers[e.type]?.(e);
  }, [frame]);
}

/** The part under the pointer this frame, and where its ray met it: the 2D parts first (drawn over the 3D), the top
 * one by z (the later of two at one z), then the 3D through the camera. */
function pickFrame(p: { x: number; y: number } | undefined, frame: { width: number; height: number }): [string | null, Vec3 | undefined] {
  if (!p) return [null, undefined];
  const flats = [...scene.parts].filter(([, x]) => x.flat && onFlat(p, x.flat));
  if (flats.length > 0) return [flats.reduce((a, b) => (b[1].flat!.z >= a[1].flat!.z ? b : a))[0], undefined];
  if (!scene.camera) return [null, undefined];
  // a camera in a page element: its view, and so its picking, is that element's box as last laid out
  const targets = [...scene.parts.values()].flatMap((x) => (x.target ? [x.target] : []));
  const hit = pickIn(p, scene.camera, targets, frame, viewBox());
  return [hit?.name ?? null, hit?.point];
}

// a host older than page.viewBox has no box to give: its cameras draw under the whole frame
let noViewBox = false;
function viewBox(): number[] | null {
  if (noViewBox || !hasNative()) return null;
  try {
    return (native as (n: string, a: object) => number[] | null)("page.viewBox", {});
  } catch {
    noViewBox = true;
    return null;
  }
}

/** The part under `pointer` (frame px) through `camera`: in the box `[x, y, w, h]` the camera draws its view in when
 * it sits in a page element (none outside it), else in the whole frame. */
export function pickIn(pointer: { x: number; y: number }, camera: PickCamera, parts: readonly Pickable[], frame: { width: number; height: number }, box?: readonly number[] | null): Hit | null {
  if (!box) return pick(pointer, camera, parts, frame);
  const [x, y, w, h] = box;
  if (pointer.x < x || pointer.y < y || pointer.x >= x + w || pointer.y >= y + h) return null;
  return pick({ x: pointer.x - x, y: pointer.y - y }, camera, parts, { width: w, height: h });
}

/** Place3d's value inside a part at `m` (its parent's world matrix times its own). */
export function usePlace3d(position: Vec3, rotation: Vec3, scale: number | Vec3, q?: [number, number, number, number]): Mat4 {
  const m = under(useContext(Place3d), local(position, rotation, scale, q));
  // the same matrix while its numbers are: a new one each frame is a new Place3d value, which rebuilds every part
  // under this one, memoised or not (#160)
  const kept = useRef(m);
  if (kept.current !== m && kept.current.length === m.length && kept.current.every((v, i) => v === m[i])) return kept.current;
  return (kept.current = m);
}

/** A part's pointer handlers, as a page element's. */
export interface PointerHandlers3d {
  /** a press and release over the part */
  onClick?: (e: Event3d) => void;
  /** a button pressed over the part */
  onPointerDown?: (e: Event3d) => void;
  /** a button released over the part */
  onPointerUp?: (e: Event3d) => void;
  /** the pointer came over the part (bubbles, as the DOM's) */
  onPointerOver?: (e: Event3d) => void;
  /** the pointer left the part (bubbles) */
  onPointerOut?: (e: Event3d) => void;
  /** the pointer came over the part (doesn't bubble) */
  onPointerEnter?: (e: Event3d) => void;
  /** the pointer left the part (doesn't bubble) */
  onPointerLeave?: (e: Event3d) => void;
  /** the pointer moved over the part */
  onPointerMove?: (e: Event3d) => void;
  /** the wheel turned over the part (`deltaX`, `deltaY`) */
  onWheel?: (e: Event3d) => void;
  /** a drag began on the part pressed */
  onDragStart?: (e: Event3d) => void;
  /** the part pressed is being dragged, each frame it moves */
  onDrag?: (e: Event3d) => void;
  /** the drag of the part pressed ended */
  onDragEnd?: (e: Event3d) => void;
  /** a dragged part came over this one (`dragged` names it) */
  onDragEnter?: (e: Event3d) => void;
  /** a dragged part is over this one */
  onDragOver?: (e: Event3d) => void;
  /** a dragged part left this one */
  onDragLeave?: (e: Event3d) => void;
  /** a dragged part was dropped on this one */
  onDrop?: (e: Event3d) => void;
}
