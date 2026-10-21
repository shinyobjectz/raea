// Composition and registerRoot: record what's registered so a renderer can list compositions
// (spec: "Declaring and registering", "The clock renderers use").
import { Children, isValidElement, type ComponentType, type ReactNode } from "react";
import { Audio, Video } from "./media";
import { Region } from "./region";
import { Freeze, Loop, Sequence, Series } from "./timeline";

export interface CompositionProps<P extends Record<string, unknown> = Record<string, unknown>> {
  id: string;
  component: ComponentType<P>;
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  defaultProps?: P;
}

export interface RegisteredComposition {
  id: string;
  component: ComponentType<any>;
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  defaultProps: Record<string, unknown>;
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
  for (const field of ["width", "height", "durationInFrames"] as const) {
    const v = props[field];
    if (!Number.isInteger(v) || v <= 0) fail(field, `must be a positive integer, got ${v}`);
  }
  if (typeof props.fps !== "number" || !(props.fps > 0) || !Number.isFinite(props.fps)) {
    fail("fps", `must be a positive number, got ${props.fps}`);
  }
  const defaultProps = props.defaultProps ?? {};
  if (!isJsonLike(defaultProps, new Set()) || Array.isArray(defaultProps)) {
    fail("defaultProps", "must be an object of JSON-like values (no functions, class instances or cycles)");
  }
  return {
    id: props.id,
    component: props.component,
    width: props.width,
    height: props.height,
    fps: props.fps,
    durationInFrames: props.durationInFrames,
    defaultProps,
  };
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

function looksHooked(fn: Function): boolean {
  if (HOOKED.has(fn)) return true;
  let source = "";
  try {
    source = Function.prototype.toString.call(fn);
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
export function registerRoot(Root: ComponentType): void {
  if (root) throw new Error("registerRoot was already called; a program has exactly one root");
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
      defaultProps: {},
    });
  }
  root = Root;
  compositions = walk.found;
}

/** The root registered with registerRoot, or null. */
export function getRoot(): ComponentType | null {
  return root;
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
