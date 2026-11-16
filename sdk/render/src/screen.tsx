// <Screen>: a live picture on a surface in the 3D scene (a CRT on a desk, a robot's face, a TV in a room). The picture
// is a nested composition or game, laid out on the page at its own size (useVideoConfig inside it says that size)
// but out of the frame's view, and painted on its own into a texture each frame, which raea-render draws on a quad
// or on a model's material as a CRT (engine/render/src/engine/screens.rs, runtime/page/src/screen.rs). The same page drives a
// render and a live play, so the screen shows the same picture on the same frame in both, and a game on a screen
// plays live: it reads the same useInput as everything else.
import { isValidElement, type ReactElement, type ReactNode } from "react";
import { Sequence } from "@raea/video/timeline";
import { Video } from "@raea/video/media";

type Vec3 = [number, number, number];

/** How the tube draws the picture. Every value is optional; `crt={false}` on the Screen draws it flat and sharp. */
export interface CrtSettings {
  /** barrel distortion, 0 (none) to about 0.3: a flat quad's stand-in for curved glass (a model's curved glass curves
   * the picture already; 0 by default) */
  curvature?: number;
  /** how dark the gaps between scanlines are, 0 to 1 (0.55); they fade where they'd be finer than the frame's pixels */
  scanlines?: number;
  /** how strongly the phosphor mask shows, 0 to 1 (0.35) */
  mask?: number;
  /** "aperture" (a Trinitron's vertical stripes, the default), "shadow" (offset triads) or "none" */
  maskKind?: "aperture" | "shadow" | "none";
  /** mask triads per source pixel (1), or per line when `lines` is set */
  maskSize?: number;
  /** the tube's own scanlines, top to bottom (0, the default: one per row of the picture). Set it when the picture is
   * sharper than the tube: a 1440x1044 desktop on a 1702 monitor shows its scanlines and mask at `lines: 270`, where
   * its own 1044 rows are finer than the frame's pixels and fade out */
  lines?: number;
  /** the phosphor's halo around bright parts, 0 to 1 (0.35) */
  glow?: number;
  /** how far apart the beam's red and blue land, in source pixels, growing toward the edges (0.35) */
  convergence?: number;
  /** the tube's edges and corners darkened, 0 to 1 (0.35) */
  vignette?: number;
  /** brightness flicker and a faint rolling hum bar, seeded by the frame, 0 to 1 (0.04) */
  flicker?: number;
  /** how strongly the picture glows (2.5 for a CRT, 1.4 flat): above 1 it blooms under a <Bloom> */
  brightness?: number;
  /** the dark tube's own colour, hex (a grey-green) */
  glass?: string;
  /** the glass's sheen toward grazing angles, 0 to 1 (0.5) */
  reflect?: number;
  /** fingerprints, a wiped arc and dust specks on the glass, 0 (clean, the default) to 1: seen mostly in the sheen
   * and as a faint lift of the dark glass, barely over the picture; the pattern is fixed per screen (seeded by its
   * name), so it holds still from frame to frame. 0.3 is a lived-in set seen on a second look */
  smudge?: number;
  /** the picture grown past the glass's edge, 0 to 0.3 (0) */
  overscan?: number;
}

export interface ScreenSource {
  /** the picture's size in pixels, [width, height] ([320, 200]); the nested composition lays out at it */
  resolution?: [number, number];
  /** draw it as a CRT (true, the default; or its settings) or flat (false: an LCD) */
  crt?: boolean | CrtSettings;
  /** the tube's power, 0 (off) to 1 (on, the default): animate it to switch the set on, a line opening to the picture */
  power?: number;
  /** the frame of this Sequence the nested composition's frame 0 falls on (0) */
  from?: number;
  /** behind the picture ("#000000"). A see-through one ("#ffffff00", "transparent", rgba(..., 0.5)) makes the screen
   * see-through where its page paints nothing, so what's behind it in the scene shows (a flat screen: a layer of a
   * page pulled apart in 3D); an opaque one keeps the screen opaque, which writes depth for DepthOfField */
  background?: string;
  /** a flat screen's (`crt={false}`) light: 1, the default, draws its page at the page's own colours, unlit (scene
   * lights don't touch it); above 1 it glows, and blooms under a <Bloom>. A tone curve on the camera bends it like
   * the rest of the frame: give the scene <ToneMapping mode="none" /> to keep the page's colours exact. A CRT's is
   * `crt.brightness` */
  brightness?: number;
  /** a video to show instead of children: staticFile("clip.mp4"), filling the picture */
  src?: string;
  /** an app to show instead of children: a module's path from the composition's folder ("chat.tsx"), its default
   * export a React component given {width, height}. It runs on react-dom (@raea/ui's) in a Blitz document of its
   * own, so npm's React libraries work on it (assistant-ui, Radix; installed with `raea add`). In play the pointer
   * reaches it through the 3D, where it lands on the picture, and a press on it gives it the keys (the game doesn't
   * get them) until a press elsewhere or Escape; in a render it runs on the frame's clock, so the render repeats */
  app?: string;
  /** the nested composition or game */
  children?: ReactNode;
}

export interface ScreenProps extends ScreenSource {
  /** what the screen is called; screens on one page need different names ("screen" by default) */
  name?: string;
  /** a free-standing screen's place: a quad of `size` metres ([1.6] wide by default, its height from the resolution) */
  position?: Vec3;
  /** radians */
  rotation?: Vec3;
  /** its scale: one number, or x, y and z */
  scale?: number | Vec3;
  /** [width] or [width, height] in metres ([1.6], its height from the resolution) */
  size?: [number] | [number, number];
  /** the composition pass it draws in (Mesh's `pass`; "scene" by default): `pass="ui"` with <Look skip={["ui"]}> keeps
   * a page crisp under a paint look */
  pass?: string;
}

const NAME_OK = /^[A-Za-z0-9_.:-]{1,64}$/;

/** A screen's spec as the engine's rows take it ({name, w, h, crt}), checked. */
export function screenSpec(owner: string, name: string, p: ScreenSource): { name: string; w: number; h: number; crt: boolean | CrtSettings; alpha?: true; brightness?: number } {
  if (!NAME_OK.test(name)) throw new TypeError(`${owner}: a screen's name is 1-64 letters, digits, _ . : or -, got ${JSON.stringify(name)}`);
  const [w, h] = p.resolution ?? [320, 200];
  if (!(Number.isInteger(w) && Number.isInteger(h) && w >= 8 && h >= 8 && w <= 4096 && h <= 4096)) throw new RangeError(`${owner}: resolution is [width, height] in whole pixels, 8 to 4096, got ${JSON.stringify(p.resolution)}`);
  if (p.power !== undefined && !(p.power >= 0 && p.power <= 1)) throw new RangeError(`${owner}: power is from 0 to 1, got ${p.power}`);
  const crt = p.crt ?? true;
  if (typeof crt === "object") {
    for (const [k, v] of Object.entries(crt)) {
      if (k === "maskKind") {
        if (!["aperture", "shadow", "none"].includes(v as string)) throw new RangeError(`${owner}: crt.maskKind is "aperture", "shadow" or "none", got ${JSON.stringify(v)}`);
      } else if (k === "glass") {
        if (typeof v !== "string" || !/^#[0-9a-fA-F]{6}$/.test(v)) throw new TypeError(`${owner}: crt.glass is a hex colour like "#1a1d1b", got ${JSON.stringify(v)}`);
      } else if (k === "smudge") {
        if (!(typeof v === "number" && v >= 0 && v <= 1)) throw new RangeError(`${owner}: crt.smudge is from 0 to 1, got ${JSON.stringify(v)}`);
      } else if (!(typeof v === "number" && Number.isFinite(v) && v >= 0)) throw new RangeError(`${owner}: crt.${k} must be a number 0 or more, got ${JSON.stringify(v)}`);
    }
  } else if (typeof crt !== "boolean") throw new TypeError(`${owner}: crt is true, false or the tube's settings, got ${JSON.stringify(crt)}`);
  if (p.brightness !== undefined) {
    if (!(typeof p.brightness === "number" && Number.isFinite(p.brightness) && p.brightness >= 0)) throw new RangeError(`${owner}: brightness is a number 0 or more, got ${JSON.stringify(p.brightness)}`);
    if (crt !== false) throw new TypeError(`${owner}: brightness is a flat screen's (crt={false}); a CRT's is crt.brightness`);
  }
  if (p.background !== undefined && typeof p.background !== "string") throw new TypeError(`${owner}: background is a CSS colour, got ${JSON.stringify(p.background)}`);
  return { name, w, h, crt, ...(seeThrough(p.background) ? { alpha: true as const } : {}), ...(p.brightness !== undefined ? { brightness: p.brightness } : {}) };
}

/** Whether a CSS colour lets what's behind it show: "transparent", #rgba / #rrggbbaa under full alpha, rgba()/hsla()
 * (or the space-separated "/ a" form) under 1. Named and other colours are opaque. */
export function seeThrough(c: string | undefined): boolean {
  if (c === undefined) return false;
  const v = c.trim().toLowerCase();
  if (v === "transparent") return true;
  if (/^#[0-9a-f]{4}$/.test(v)) return v[4] !== "f";
  if (/^#[0-9a-f]{8}$/.test(v)) return v.slice(7) !== "ff";
  const m = v.match(/^(?:rgba?|hsla?|oklch|oklab|lab|lch|hwb)\((.*)\)$/);
  if (!m) return false;
  const parts = m[1].includes("/") ? [m[1].split("/")[1]] : m[1].split(",").length === 4 ? [m[1].split(",")[3]] : [];
  if (parts.length === 0) return false;
  const a = parts[0].trim();
  const n = a.endsWith("%") ? parseFloat(a) / 100 : parseFloat(a);
  return Number.isFinite(n) && n < 1;
}

/** The picture's page part: the nested composition at its own size, out of the frame's view, marked for the screen
 * pass. Its own time (a Sequence from `from`, at the picture's size), so useVideoConfig inside says the picture's size. */
export function screenContent(name: string, p: ScreenSource) {
  const [w, h] = p.resolution ?? [320, 200];
  if (p.app !== undefined && (typeof p.app !== "string" || p.app === "")) throw new TypeError(`Screen ${name}: app is a module's path from the composition's folder, like "chat.tsx", got ${JSON.stringify(p.app)}`);
  const body = p.app !== undefined
    ? null
    : p.src !== undefined
    ? <Video src={p.src} style={{ width: w, height: h, objectFit: "cover" }} />
    : p.children;
  return (
    <div
      key={`screen-${name}`}
      data-raea-screen={name}
      data-raea-screen-w={w}
      data-raea-screen-h={h}
      {...(p.app !== undefined ? { "data-raea-screen-app": p.app } : {})}
      style={{ position: "absolute", left: -20000, top: 0, width: w, height: h, overflow: "hidden", backgroundColor: p.background ?? "#000000" }}
    >
      <Sequence from={p.from ?? 0} width={w} height={h} name={`screen ${name}`}>{body}</Sequence>
    </div>
  );
}

/** A Model's `screens` prop: each material's screen, as a <Screen> element (its props and children) or a bare
 * composition element (a <Screen> of the defaults around it). */
export type ModelScreens = Record<string, ReactElement>;

/** Each `screens` entry's spec for the model's row (material, name, w, h, crt) and its page part. */
export function modelScreens(owner: string, model: string, screens: ModelScreens): { specs: object[]; parts: ReactNode[] } {
  const specs: object[] = [];
  const parts: ReactNode[] = [];
  for (const [material, el] of Object.entries(screens)) {
    if (!isValidElement(el)) throw new TypeError(`${owner}: screens.${material} must be an element: <Screen resolution={[320, 200]}><Game /></Screen> or <Game />`);
    const isScreen = el.type === Screen;
    const sp = (isScreen ? el.props : { children: el }) as ScreenProps;
    const name = sp.name ?? `${model}:${material}`;
    specs.push({ material, ...screenSpec(owner, name, sp) });
    parts.push(screenContent(name, sp));
  }
  return { specs, parts };
}

/** A live picture on a surface in the 3D scene: a nested composition or game (its children, or `src`, a video) at its
 * own resolution, drawn as a CRT (scanlines, mask, glow, the glass) or flat. Inside a Canvas3D on its own it's a
 * free-standing quad; on a model, give it to the Model: `<Model src={...} screens={{ monitor_screen: <Screen
 * resolution={[384, 272]}><Game /></Screen> }} />`. A screen doesn't light the room: put a dim light of its colour in
 * front of it.
 * @bevy bevy_pbr::mesh_material::MeshMaterial3d<raea_render::engine::screen::CrtMaterial> */
export function Screen(props: ScreenProps) {
  const name = props.name ?? "screen";
  const spec = screenSpec("Screen", name, props);
  if (props.pass !== undefined && !(typeof props.pass === "string" && /^[A-Za-z][A-Za-z0-9_-]{0,31}$/.test(props.pass))) throw new TypeError(`Screen: pass is a name like "ui" (letters first, up to 32 letters, digits, _ or -), got ${JSON.stringify(props.pass)}`);
  const size = props.size ?? [1.6];
  if (!(Array.isArray(size) && size.length >= 1 && size.every((v) => typeof v === "number" && v > 0))) throw new RangeError(`Screen: size is [width] or [width, height] in metres, got ${JSON.stringify(props.size)}`);
  const width = size[0];
  const height = size[1] ?? (width * spec.h) / spec.w;
  const { children: _c, src: _s, app: _a, resolution: _r, crt: _k, power, from: _f, background: _b, brightness: _l, size: _z, ...place } = props;
  const p = { ...place, name, width, height, screen: spec, ...(power !== undefined ? { screenPower: power } : {}) };
  return (
    <>
      <div data-3d="Screen" data-props={JSON.stringify(p)} />
      {screenContent(name, props)}
    </>
  );
}

/** Whether a node is one of Screen's (for tools that walk a tree). */
export function isScreen(el: unknown): boolean {
  return isValidElement(el) && el.type === Screen;
}

