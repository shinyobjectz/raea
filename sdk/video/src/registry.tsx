// Composition and registerRoot: record what's registered so a renderer can list compositions
// (spec: "Declaring and registering", "The clock renderers use").
import { Children, isValidElement, type ComponentType, type ReactElement, type ReactNode } from "react";
import { useVideoConfig } from "@raea/app/clock";
import { Audio, Video } from "./media";
import { Region } from "./region";
import { Freeze, Loop, Sequence, Series } from "./timeline";
import { validControls, type Control, type Controls, type ControlCost, type RegisteredControl } from "./controls";
export type { ChoiceControl, ColourControl, Control, ControlCost, Controls, NumberControl, RegisteredControl, SwitchControl, TextControl } from "./controls";

export interface CompositionProps<P extends Record<string, unknown> = Record<string, unknown>> {
  /** the composition's id: letters, digits and dashes (`raea render --composition` picks it) */
  id: string;
  /** the React component it renders */
  component: ComponentType<P>;
  /** in px */
  width: number;
  /** in px */
  height: number;
  /** frames a second */
  fps: number;
  /** its length in frames */
  durationInFrames: number;
  /** the props it's rendered with (JSON-like values only) */
  defaultProps?: P;
  /** its size, rate, length or props worked out from the props it's rendered with (`defaultProps` under the command
   *  line's `--props`): what it returns replaces the values above, so one composition renders a video per props file.
   *  It returns its values directly, not a promise */
  calculateMetadata?: CalculateMetadata<P>;
  /** the composition's controls: which of its defaultProps a person may tune, and how (docs/realtime.feature RT4).
   *  Each names a defaultProps key, whose value is the control's; the feed's Controls panel shows them on the
   *  timeline and a change reaches the live picture as that prop */
  controls?: Controls<P>;
  /** a hand edit laid over the composition (docs/craft-system.md, L): written back by the app's timeline */
  edit?: Edit;
}

/** What `calculateMetadata` is given and may return. */
export type CalculateMetadata<P extends Record<string, unknown> = Record<string, unknown>> = (given: { defaultProps: P; props: P; compositionId: string }) => {
  durationInFrames?: number;
  width?: number;
  height?: number;
  fps?: number;
  props?: P;
} | void;

/** The props given on the command line (`raea render --props '{"n": 2}'` or `--props props.json`), anywhere in a
 *  program; `{}` when none were. A composition's component gets them over its `defaultProps`. */
export function getInputProps<T extends Record<string, unknown> = Record<string, unknown>>(): T {
  return ((globalThis as { __raea_input_props?: unknown }).__raea_input_props ?? {}) as T;
}

/** A hand edit: `segments` are the composition's own frame ranges, [from, to), played one after another, so a trim,
 *  a split, a cut word or a moved span is a change to this list; the length becomes their sum. */
export interface Edit {
  /** the composition's own frame ranges, [from, to), played one after another */
  segments?: readonly (readonly [number, number])[];
  /** the composition drawn at this fraction of its size (above 0, at most 1), what is where kept: it is laid out at
   *  its width and height (useVideoConfig still says them) and the whole drawn smaller, so a 3D view traces fewer
   *  pixels. A draft's, where a smaller picture changes nothing that ships: the app's composition drafts and a scene's
   *  kept frames draw at 0.5 (docs/lightspeed.feature PF4) */
  scale?: number;
}

export interface RegisteredComposition {
  /** the composition's id: letters, digits and dashes (`raea render --composition` picks it) */
  id: string;
  /** the React component it renders */
  component: ComponentType<any>;
  /** in px */
  width: number;
  /** in px */
  height: number;
  /** frames a second */
  fps: number;
  /** its length in frames */
  durationInFrames: number;
  /** the props it's rendered with (JSON-like values only) */
  defaultProps: Record<string, unknown>;
  /** its controls, by defaultProps key, in the order they're declared ({} when it has none) */
  controls: Readonly<Record<string, RegisteredControl>>;
}

const ID_PATTERN = /^[A-Za-z0-9-]+$/;

function isJsonLike(v: unknown, seen: Set<unknown>): boolean {
  if (v === null || typeof v === "string" || typeof v === "boolean") return true;
  if (typeof v === "number") return Number.isFinite(v);
  if (typeof v !== "object") return false;
  if (seen.has(v)) return false;
  seen.add(v);
  const ok = Array.isArray(v)
    ? v.every((x) => isJsonLike(x, seen))
    : Object.getPrototypeOf(v) === Object.prototype && Object.values(v).every((x) => isJsonLike(x, seen));
  seen.delete(v);
  return ok;
}

function validate(props: CompositionProps<any>, taken: readonly RegisteredComposition[]): RegisteredComposition {
  const name = typeof props.id === "string" && props.id !== "" ? `Composition "${props.id}"` : "A Composition";
  const fail = (field: string, why: string): never => {
    throw new Error(`${name}: ${field} ${why}`);
  };
  if (typeof props.id !== "string" || !ID_PATTERN.test(props.id)) {
    fail("id", `must be letters, digits and hyphens, got ${JSON.stringify(props.id)}`);
  }
  if (taken.some((c) => c.id === props.id)) fail("id", "is already used by another composition in this root");
  if (typeof props.component !== "function" && (typeof props.component !== "object" || props.component === null)) {
    fail("component", "must be a React component");
  }
  const jsonObject = (field: string, v: unknown) => {
    if (!isJsonLike(v, new Set()) || Array.isArray(v)) fail(field, "must be an object of JSON-like values (no functions, class instances or cycles)");
  };
  const declared = props.defaultProps ?? {};
  jsonObject("defaultProps", declared);
  // the command line's props over its own (--props, MV4), then what calculateMetadata makes of them
  const input = getInputProps();
  let defaultProps: Record<string, unknown> = Object.keys(input).length ? { ...declared, ...input } : declared;
  if (props.calculateMetadata !== undefined) {
    if (typeof props.calculateMetadata !== "function") fail("calculateMetadata", "must be a function");
    const meta = props.calculateMetadata({ defaultProps: declared, props: defaultProps, compositionId: props.id });
    if (meta && typeof (meta as { then?: unknown }).then === "function") fail("calculateMetadata", "returned a promise: it returns its values directly (pass data it would load with --props)");
    if (meta) {
      const { props: made, ...size } = meta;
      // (a copy: an element's props are frozen)
      props = { ...props, ...Object.fromEntries(Object.entries(size).filter(([, v]) => v !== undefined)) };
      if (made !== undefined) {
        jsonObject("calculateMetadata's props", made);
        defaultProps = made;
      }
    }
  }
  for (const field of ["width", "height", "durationInFrames"] as const) {
    const v = props[field];
    if (!Number.isInteger(v) || v <= 0) fail(field, `must be a positive integer, got ${v}`);
  }
  if (typeof props.fps !== "number" || !(props.fps > 0) || !Number.isFinite(props.fps)) {
    fail("fps", `must be a positive number, got ${props.fps}`);
  }
  const segments = props.edit?.segments;
  if (segments !== undefined) {
    if (!Array.isArray(segments) || segments.length === 0) fail("edit.segments", "must be a non-empty list of [from, to] frame ranges");
    for (const seg of segments) {
      const ok = Array.isArray(seg) && seg.length === 2 && Number.isInteger(seg[0]) && Number.isInteger(seg[1]) && seg[0] >= 0 && seg[1] > seg[0] && seg[1] <= props.durationInFrames;
      if (!ok) fail("edit.segments", `has ${JSON.stringify(seg)}: each is [from, to) with 0 <= from < to <= ${props.durationInFrames}`);
    }
  }
  const draft = props.edit?.scale;
  if (draft !== undefined && !(typeof draft === "number" && draft > 0 && draft <= 1)) fail("edit.scale", `must be a number above 0 and at most 1, got ${draft}`);
  // with the command line's --scale (MV5): laid out at its own size, drawn at that many times it
  const scale = (draft ?? 1) * ((globalThis as { __raea_scale?: number }).__raea_scale ?? 1);
  const [w, h] = scale === 1 ? [props.width, props.height] : [Math.max(1, Math.round(props.width * scale)), Math.max(1, Math.round(props.height * scale))];
  const controls = validControls(props.controls, defaultProps as Record<string, unknown>, fail);
  // --hold N:M: frame N drawn M times, so what still changes on screen is the renderer's own (raea light steady)
  const hold = (globalThis as { __raea_hold?: [number, number] }).__raea_hold;
  const Whole = segments ? edited(props.component, segments) : props.component;
  const cut: ComponentType<any> = hold ? (p) => <Freeze frame={hold[0]}><Whole {...p} /></Freeze> : Whole;
  return {
    id: props.id,
    component: w === props.width && h === props.height ? cut : scaled(cut, props.width, props.height, w, h),
    width: w,
    height: h,
    fps: props.fps,
    durationInFrames: hold ? hold[1] : segments ? segments.reduce((n, [a, b]) => n + b - a, 0) : props.durationInFrames,
    defaultProps,
    controls,
  };
}

/** The component played through an edit's segments: each one a Sequence at its place in the cut, holding the
 *  composition shifted so its own frame `from` lands there. Sound follows, since it's placed by the same Sequences. */
function edited(Component: ComponentType<any>, segments: readonly (readonly [number, number])[]): ComponentType<any> {
  const at: number[] = [];
  segments.reduce((n, [a, b]) => (at.push(n), n + b - a), 0);
  function Edited(props: Record<string, unknown>) {
    return (
      <>
        {segments.map(([a, b], i) => (
          <Sequence key={i} from={at[i]} durationInFrames={b - a} layout="none">
            <Sequence from={-a} layout="none">
              <Component {...props} />
            </Sequence>
          </Sequence>
        ))}
      </>
    );
  }
  Edited.displayName = `Edited(${Component.displayName ?? Component.name ?? "Composition"})`;
  return Edited;
}

/** The component laid out at its own size (`width` by `height`, which useVideoConfig reports inside it) in a box
 *  drawn at `w` by `h`: an edit's scale. */
function scaled(Component: ComponentType<any>, width: number, height: number, w: number, h: number): ComponentType<any> {
  const style = { position: "absolute", left: 0, top: 0, width, height, transform: `scale(${w / width}, ${h / height})`, transformOrigin: "0 0" } as const;
  function Scaled(props: Record<string, unknown>) {
    return (
      <div style={style}>
        <Sequence layout="none" width={width} height={height}>
          <Component {...props} />
        </Sequence>
      </div>
    );
  }
  Scaled.displayName = `Scaled(${Component.displayName ?? Component.name ?? "Composition"})`;
  return Scaled;
}

/** A control's value as the frame is drawn (docs/realtime.feature RT4c): the composition's prop of that name, the
 *  value a person set on the live stage or, in a render, its defaultProps. For a part deep in the tree, so the value
 *  needn't be passed down as a prop; the root component can read its props as any component does. */
export function useControl<T = unknown>(name: string): T {
  const { defaultProps } = useVideoConfig();
  if (!Object.prototype.hasOwnProperty.call(defaultProps, name)) {
    const keys = Object.keys(defaultProps);
    throw new Error(`useControl("${name}"): the composition has no prop "${name}" (its defaultProps has ${keys.length ? keys.join(", ") : "none"})`);
  }
  return defaultProps[name] as T;
}

/** Declares a composition inside the root. It draws nothing itself; a renderer finds it through registerRoot. */
export function Composition<P extends Record<string, unknown>>(_props: CompositionProps<P>) {
  return null;
}

// Components of this package whose rendering reads the clock (they use hooks), so the walk never calls them.
const HOOKED: ReadonlySet<unknown> = new Set<unknown>([
  Sequence, Series, Series.Sequence, Loop, Freeze, Region, Video, Audio,
]);

// A call to a hook in a component's source: `use(` or `useSomething(` (also after a dot, as bundlers write it).
const HOOK_CALL = /\buse(?:[A-Z][\w$]*)?\s*\(/;

const CLOSE: Record<string, string> = { "(": ")", "[": "]", "{": "}" };
// what a `/` follows when it starts a regex literal rather than dividing
const REGEX_AFTER = /[(,=:[!&|?{};+\-*%<>~^]/;
const REGEX_WORD = /\b(?:return|typeof|case|do|else|in|of|new|delete|void|throw|yield|await)$/;

/** Where the string, template literal, comment or regex literal at `i` ends (the index after it); -1 when it's code. */
function opaqueEnd(s: string, i: number): number {
  const c = s[i];
  if (c === '"' || c === "'") {
    for (let j = i + 1; j < s.length; j++) {
      if (s[j] === "\\") j++;
      else if (s[j] === c || s[j] === "\n") return j + 1;
    }
    return s.length;
  }
  if (c === "`") {
    for (let j = i + 1; j < s.length; j++) {
      if (s[j] === "\\") j++;
      else if (s[j] === "`") return j + 1;
      else if (s[j] === "$" && s[j + 1] === "{") j = closeOf(s, j + 1) - 1;
    }
    return s.length;
  }
  if (c !== "/") return -1;
  if (s[i + 1] === "/") {
    const e = s.indexOf("\n", i);
    return e < 0 ? s.length : e + 1;
  }
  if (s[i + 1] === "*") {
    const e = s.indexOf("*/", i + 2);
    return e < 0 ? s.length : e + 2;
  }
  let k = i - 1;
  while (k >= 0 && /\s/.test(s[k])) k--;
  if (k >= 0 && !REGEX_AFTER.test(s[k]) && !REGEX_WORD.test(s.slice(Math.max(0, k - 7), k + 1))) return -1; // division
  let inClass = false;
  for (let j = i + 1; j < s.length; j++) {
    if (s[j] === "\\") j++;
    else if (s[j] === "[") inClass = true;
    else if (s[j] === "]") inClass = false;
    else if (s[j] === "/" && !inClass) {
      let end = j + 1;
      while (end < s.length && /[a-z]/i.test(s[end])) end++;
      return end;
    }
    else if (s[j] === "\n") return j + 1;
  }
  return s.length;
}

/** The index after the bracket that closes the one at `i`, past strings, comments and the like. */
function closeOf(s: string, i: number): number {
  let depth = 0;
  for (let j = i; j < s.length; ) {
    const e = opaqueEnd(s, j);
    if (e >= 0) {
      j = e;
      continue;
    }
    if (CLOSE[s[j]]) depth++;
    else if (s[j] === ")" || s[j] === "]" || s[j] === "}") {
      depth--;
      if (depth === 0) return j + 1;
    }
    j++;
  }
  return s.length;
}

/** Where an arrow function's expression body starting at `i` ends: at a `,`, `;` or closing bracket outside brackets. */
function expressionEnd(s: string, i: number): number {
  for (let j = i; j < s.length; ) {
    const e = opaqueEnd(s, j);
    if (e >= 0) {
      j = e;
      continue;
    }
    if (CLOSE[s[j]]) {
      j = closeOf(s, j);
      continue;
    }
    if (s[j] === "," || s[j] === ";" || s[j] === ")" || s[j] === "]" || s[j] === "}") return j;
    j++;
  }
  return s.length;
}

/**
 * A function's source without the bodies of the functions written inside it: a component given inline
 * (`component={() => ...}`), a render prop or a callback runs when React or the caller runs it, not when this function
 * is called, so a hook there isn't this function's. Each inner `=>` body and `function` body becomes empty.
 */
function ownSource(s: string): string {
  // this function's own body: after its first `=>`, or inside its first `{`, outside its parameters' brackets
  let body = -1;
  for (let j = 0; j < s.length && body < 0; ) {
    const e = opaqueEnd(s, j);
    if (e >= 0) j = e;
    else if (s.startsWith("=>", j)) body = j + 2;
    else if (s[j] === "{") body = j + 1;
    else if (s[j] === "(" || s[j] === "[") j = closeOf(s, j);
    else j++;
  }
  if (body < 0) return s;
  let out = s.slice(0, body);
  for (let j = body; j < s.length; ) {
    const e = opaqueEnd(s, j);
    if (e >= 0) {
      out += s.slice(j, e);
      j = e;
      continue;
    }
    if (s.startsWith("=>", j)) {
      let k = j + 2;
      while (k < s.length && /\s/.test(s[k])) k++;
      out += "=>0";
      j = s[k] === "{" ? closeOf(s, k) : expressionEnd(s, k);
      continue;
    }
    if (s.startsWith("function", j) && !/[\w$]/.test(s[j - 1] ?? "") && !/[\w$]/.test(s[j + 8] ?? "")) {
      const open = s.indexOf("(", j);
      let k = open < 0 ? s.length : closeOf(s, open);
      while (k < s.length && /\s/.test(s[k])) k++;
      if (s[k] === "{") {
        out += "function(){}";
        j = closeOf(s, k);
        continue;
      }
    }
    out += s[j];
    j++;
  }
  return out;
}

function looksHooked(fn: Function): boolean {
  if (HOOKED.has(fn)) return true;
  let source = "";
  try {
    source = ownSource(Function.prototype.toString.call(fn));
  } catch {
    return true; // can't read it: don't risk running it
  }
  return HOOK_CALL.test(source) || /\bword\s*\(/.test(source);
}

/** Calls a component that shows no hooks, quietly; undefined when it fails (then it's treated as content). */
function expand(fn: (p: unknown) => ReactNode, props: unknown): { ok: true; node: ReactNode } | { ok: false } {
  const quiet = console.error;
  console.error = () => {};
  try {
    return { ok: true, node: fn(props) };
  } catch {
    return { ok: false };
  } finally {
    console.error = quiet;
  }
}

interface Walk {
  found: RegisteredComposition[];
  /** the latest end among the top-level Sequences with a finite duration */
  latestEnd: number;
}

/** The end frame of a top-level Sequence (or Series), when its duration is finite. */
function sequenceEnd(type: unknown, props: Record<string, unknown>): number | undefined {
  if (type === Sequence) {
    const from = typeof props.from === "number" ? props.from : 0;
    const d = props.durationInFrames;
    return typeof d === "number" && Number.isFinite(d) ? from + d : undefined;
  }
  if (type === Series) {
    let end = 0;
    for (const child of Children.toArray(props.children as ReactNode)) {
      if (!isValidElement(child)) continue;
      const p = child.props as { durationInFrames?: unknown; offset?: unknown };
      const d = p.durationInFrames;
      if (typeof d !== "number" || !Number.isFinite(d)) return undefined;
      end += (typeof p.offset === "number" ? p.offset : 0) + d;
    }
    return end;
  }
  return undefined;
}

/**
 * Finds the Compositions in a root's element tree without a renderer (the main entry must run in the engine's V8,
 * which has no server renderer). It never runs a component that uses hooks: there it stops and treats the element as
 * content. Other function components are expanded by calling them; fragments, providers and plain elements are
 * searched through their children.
 */
function collect(node: ReactNode, walk: Walk, depth: number, topLevel: boolean): void {
  if (depth > 200) throw new Error("registerRoot: the root's element tree is too deep to search for Compositions");
  if (node === null || node === undefined || typeof node === "boolean") return;
  if (Array.isArray(node)) {
    for (const child of node) collect(child, walk, depth + 1, topLevel);
    return;
  }
  if (!isValidElement(node)) return;
  const props = node.props as Record<string, unknown> & { children?: ReactNode };
  const type = node.type as unknown;
  if (type === Composition) {
    walk.found.push(validate(props as unknown as CompositionProps<any>, walk.found));
    return;
  }
  const end = sequenceEnd(type, props);
  if (end !== undefined && topLevel) walk.latestEnd = Math.max(walk.latestEnd, end);
  if (typeof type === "function") {
    const isClass = !!(type as { prototype?: { isReactComponent?: unknown } }).prototype?.isReactComponent;
    if (isClass || looksHooked(type)) return; // content
    const out = expand(type as (p: unknown) => ReactNode, props);
    if (out.ok) collect(out.node, walk, depth + 1, topLevel);
    return;
  }
  collect(props.children, walk, depth + 1, topLevel);
}

/** What a root with no Composition registers as: itself, at these settings. */
const IMPLICIT = { id: "main", fps: 30, width: 1080, height: 1920, durationInFrames: 300 } as const;

let root: ComponentType | null = null;
let compositions: RegisteredComposition[] = [];

/** Names the root component the renderer starts from, and records its compositions. Call it once. */
export function registerRoot(given: ComponentType | ReactElement): void {
  if (root) throw new Error("registerRoot was already called; a program has exactly one root");
  // registerRoot(<Composition ... />) is read as registerRoot(() => <Composition ... />), so an element given where the
  // component goes isn't rendered as a type (React's #130)
  const Root: ComponentType = isValidElement(given) ? () => given : given;
  if (typeof Root !== "function" && (typeof Root !== "object" || Root === null)) {
    throw new TypeError("registerRoot: give it the root component");
  }
  const walk: Walk = { found: [], latestEnd: 0 };
  // the root itself is always looked inside, unless it reads the clock (then it's all content)
  if (!looksHooked(Root)) {
    const out = typeof Root === "function" && !(Root as { prototype?: { isReactComponent?: unknown } }).prototype?.isReactComponent
      ? expand(Root as (p: unknown) => ReactNode, {})
      : { ok: false as const };
    if (out.ok) collect(out.node, walk, 0, true);
  }
  if (walk.found.length === 0) {
    walk.found.push({
      ...IMPLICIT,
      durationInFrames: walk.latestEnd > 0 ? Math.ceil(walk.latestEnd) : IMPLICIT.durationInFrames,
      component: Root,
      defaultProps: getInputProps(),
      controls: {},
    });
  }
  root = Root;
  compositions = walk.found;
}

/** The compositions the registered root declares, in order. */
export function getCompositions(): RegisteredComposition[] {
  return compositions.slice();
}

/** Forgets the registered root and its compositions (for tests; exported from the test entry only). */
export function clearRegistry(): void {
  root = null;
  compositions = [];
}
