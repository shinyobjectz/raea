// @raea/data: data-oriented TypeScript for raea (the founder, 2026-10-17: "a better typescript environment that will
// include the ecs and data oriented pieces"). Two things the rest of TypeScript makes awkward, made ordinary:
//
//   components and queries   typed components declared with storage types, and queries over them whose bindings
//                            TypeScript infers (`write` views writable, `read` views readonly, `without` filtering),
//                            over @raea/motion's ECS (Bevy's own World when the engine is there, TypeScript's
//                            tables under plain bun); structural changes go through commands applied after a system
//   tables                   column-oriented data in typed arrays with row views, for what has no entities: frames,
//                            moments, words, lines (facts.ts loads a brief's facts into them)
//
// A view from a query or a table's rows lives for one step of its loop: kept past it, it throws ("a view escaped
// its query"), so code never depends on a row being an object of its own.

import { World as MotionWorld, createWorld as motionWorld } from "../../motion/src/ecs";

// ---- storage types ----
type Kind = "f32" | "f64" | "i32" | "u8" | "u16" | "u32" | "bool" | "str" | "ent";
export interface Field<K extends Kind = Kind> { kind: K }
export const f32: Field<"f32"> = { kind: "f32" };
export const f64: Field<"f64"> = { kind: "f64" };
export const i32: Field<"i32"> = { kind: "i32" };
export const u8: Field<"u8"> = { kind: "u8" };
export const u16: Field<"u16"> = { kind: "u16" };
export const u32: Field<"u32"> = { kind: "u32" };
export const bool: Field<"bool"> = { kind: "bool" };
export const str: Field<"str"> = { kind: "str" };
/** an entity handle stored in a field */
export const ent: Field<"ent"> = { kind: "ent" };

export type Schema = Record<string, Field>;
type ValueOf<F> = F extends Field<"bool"> ? boolean : F extends Field<"str"> ? string : number;
/** a schema's values: what a component or a table row holds */
export type Values<S extends Schema> = { -readonly [K in keyof S]: ValueOf<S[K]> };

/** Coerce a value to its storage type, as the column holding it would (f32 rounds, u8 wraps, bool is a bool). */
function store(kind: Kind, v: unknown): number | string | boolean {
  switch (kind) {
    case "f32": return Math.fround(Number(v));
    case "f64": return Number(v);
    case "i32": return Number(v) | 0;
    case "u8": return Number(v) & 0xff;
    case "u16": return Number(v) & 0xffff;
    case "u32":
    case "ent": return Number(v) >>> 0;
    case "bool": return Boolean(v);
    case "str": return String(v ?? "");
  }
}

// ---- components ----
export interface Component<S extends Schema = Schema> {
  readonly name: string;
  readonly schema: S;
  readonly tag: false;
  /** a value of this component, checked and coerced to its storage types */
  of(v: Values<S>): [Component<S>, Values<S>];
}
export interface Tag {
  readonly name: string;
  readonly schema: {};
  readonly tag: true;
}

/** A component: a name the engine knows it by, and the storage type of each field. */
export function component<S extends Schema>(name: string, schema: S): Component<S> {
  const c: Component<S> = {
    name, schema, tag: false,
    of(v) {
      const out = {} as Values<S>;
      for (const k in schema) (out as any)[k] = store(schema[k].kind, (v as any)[k]);
      return [c, out];
    },
  };
  return c;
}

/** A tag: membership without fields. */
export function tag(name: string): Tag {
  return { name, schema: {}, tag: true };
}

// ---- query terms ----
export interface Read<C extends Component> { readonly t: "read"; readonly c: C }
export interface Write<C extends Component> { readonly t: "write"; readonly c: C }
export interface Without { readonly t: "without"; readonly c: Component | Tag }
export interface With { readonly t: "with"; readonly c: Component | Tag }
export interface EntityTerm { readonly t: "entity" }
export type Term = Read<any> | Write<any> | Without | With | EntityTerm;

export const read = <C extends Component>(c: C): Read<C> => ({ t: "read", c });
export const write = <C extends Component>(c: C): Write<C> => ({ t: "write", c });
export const without = (c: Component | Tag): Without => ({ t: "without", c });
/** an entity must have it, without binding it (a tag, or a component the loop doesn't read) */
export const has = (c: Component | Tag): With => ({ t: "with", c });
/** binds the entity's handle */
export const entity: EntityTerm = { t: "entity" };

type ViewOf<T> = T extends Read<Component<infer S>> ? Readonly<Values<S>>
  : T extends Write<Component<infer S>> ? Values<S>
  : T extends EntityTerm ? number
  : never;
/** the bindings a query's terms give, filters (without, has) left out */
export type Bindings<Ts extends readonly Term[]> =
  Ts extends readonly [infer H, ...infer R extends readonly Term[]]
    ? ViewOf<H> extends never ? Bindings<R> : [ViewOf<H>, ...Bindings<R>]
    : [];

// ---- views that live for one step ----
let generation = 0;

function view<S extends Schema>(schema: S, get: (k: string) => unknown, set: ((k: string, v: unknown) => void) | null, gen: () => number, born: number, what: string): Values<S> {
  const o: any = {};
  for (const k of Object.keys(schema)) {
    Object.defineProperty(o, k, {
      enumerable: true,
      get() {
        if (gen() !== born) throw new Error(`a view escaped its query: ${what}.${k} read after its row was left (copy() it to keep it)`);
        return get(k);
      },
      set(v) {
        if (gen() !== born) throw new Error(`a view escaped its query: ${what}.${k} written after its row was left`);
        if (!set) throw new Error(`${what} is read-only in this query (ask for write(${what}))`);
        set(k, store(schema[k].kind, v));
      },
    });
  }
  return o;
}

/** A view's values as a plain object of its own, to keep past its row. */
export function copy<T extends object>(v: T): T {
  return { ...v };
}

// ---- the world ----
type Spawnable = [Component<any>, Values<any>] | Tag;

/** Structural changes made during a system, applied when it ends. */
export class Commands {
  readonly ops: (() => void)[] = [];
  constructor(private readonly w: Data) {}
  spawn(...parts: Spawnable[]): void { this.ops.push(() => { this.w.spawn(...parts); }); }
  despawn(e: number): void { this.ops.push(() => this.w.motion.despawn(e)); }
  insert(e: number, ...parts: Spawnable[]): void { this.ops.push(() => this.w.insert(e, ...parts)); }
  remove(e: number, ...cs: (Component<any> | Tag)[]): void { this.ops.push(() => this.w.motion.remove(e, ...cs.map((c) => c.name))); }
}

export interface SystemInfo { name: string; reads: string[]; writes: string[] }

/** A typed layer over @raea/motion's World: components by declaration, queries by terms, systems by access. */
export class Data {
  readonly systems: SystemInfo[] = [];
  constructor(readonly motion: MotionWorld) {}

  /** A new entity with these components (`Position.of({x, y})`) and tags; its handle. */
  spawn(...parts: Spawnable[]): number {
    return this.motion.add(this.parts(parts));
  }
  insert(e: number, ...parts: Spawnable[]): void {
    this.motion.insert(e, this.parts(parts));
  }
  private parts(parts: Spawnable[]): Record<string, unknown> {
    const comps: Record<string, unknown> = {};
    for (const p of parts) {
      if (Array.isArray(p)) comps[p[0].name] = p[1];
      else comps[p.name] = true;
    }
    return comps;
  }

  /** One entity's component, or undefined when it hasn't it. */
  get<S extends Schema>(e: number, c: Component<S>): Values<S> | undefined {
    return this.motion.components(e)[c.name] as Values<S> | undefined;
  }

  /**
   * The entities with every bound and `has` component and none of the `without`, with views of what's bound, in the
   * order of the terms. A `write` view's changes go back to the world as the loop leaves its row.
   */
  *query<const Ts extends readonly Term[]>(...terms: Ts): Generator<Bindings<Ts>> {
    const need = terms.filter((t) => t.t === "read" || t.t === "write" || t.t === "with").map((t) => (t as any).c.name as string);
    const not = terms.filter((t) => t.t === "without").map((t) => (t as Without).c.name);
    const ids = this.motion.find({ with: need, without: not });
    for (const id of ids) {
      const comps = this.motion.components(id) as Record<string, any>;
      const born = ++generation;
      const changed = new Set<string>();
      const out: unknown[] = [];
      for (const t of terms) {
        if (t.t === "entity") out.push(id);
        else if (t.t === "read" || t.t === "write") {
          const name = t.c.name;
          const vals = { ...(comps[name] ?? {}) };
          out.push(view(t.c.schema, (k) => vals[k], t.t === "write" ? (k, v) => { vals[k] = v; changed.add(name); comps[name] = vals; } : null, () => generation, born, name));
        }
      }
      yield out as Bindings<Ts>;
      generation++;
      if (changed.size) {
        const upd: Record<string, unknown> = {};
        for (const n of changed) upd[n] = comps[n];
        this.motion.insert(id, upd);
      }
    }
  }

  /**
   * A system: run on every frame the world steps (motion's `system`, so `world.at(frame)` is the same whenever it's
   * asked for), with commands applied after it. Its reads and writes are declared for whoever schedules systems.
   */
  system(name: string, access: { reads?: (Component<any> | Tag)[]; writes?: (Component<any> | Tag)[] }, fn: (w: Data, frame: number, commands: Commands) => void): this {
    this.systems.push({ name, reads: (access.reads ?? []).map((c) => c.name), writes: (access.writes ?? []).map((c) => c.name) });
    // motion replays systems on a copy of the world for `at(frame)`: run against the world it hands over
    this.motion.system((m, frame) => {
      const d = m === this.motion ? this : new Data(m);
      const cmd = new Commands(d);
      fn(d, frame, cmd);
      for (const op of cmd.ops) op();
    });
    return this;
  }

  /** Run `fn` once now, with its commands applied after it (a system outside the frame loop). */
  run(fn: (w: Data, commands: Commands) => void): void {
    const cmd = new Commands(this);
    fn(this, cmd);
    for (const op of cmd.ops) op();
  }
}

/** A typed world over @raea/motion's (native when the engine is there). */
export function world(opts: { native?: boolean } = {}): Data {
  return new Data(motionWorld(opts));
}

// ---- tables ----
type Column = Float32Array | Float64Array | Int32Array | Uint8Array | Uint16Array | Uint32Array | string[];

function columnFor(kind: Kind, n: number): Column {
  switch (kind) {
    case "f32": return new Float32Array(n);
    case "f64": return new Float64Array(n);
    case "i32": return new Int32Array(n);
    case "u8":
    case "bool": return new Uint8Array(n);
    case "u16": return new Uint16Array(n);
    case "u32":
    case "ent": return new Uint32Array(n);
    case "str": return new Array<string>(n).fill("");
  }
}

/** Rows of a schema stored as columns: one typed array per field (strings in a plain array), grown as rows come. */
export class Table<S extends Schema> {
  private cols: Record<string, Column> = {};
  private cap = 0;
  length = 0;
  constructor(readonly schema: S, readonly name = "table") {
    this.grow(16);
  }
  private grow(n: number) {
    for (const k of Object.keys(this.schema)) {
      const kind = this.schema[k].kind;
      const next = columnFor(kind, n);
      const prev = this.cols[k];
      if (prev) {
        if (Array.isArray(prev)) for (let i = 0; i < this.length; i++) (next as string[])[i] = prev[i];
        else (next as any).set(prev.subarray(0, this.length));
      }
      this.cols[k] = next;
    }
    this.cap = n;
  }
  /** A row added; its index. */
  push(row: Values<S>): number {
    if (this.length === this.cap) this.grow(this.cap * 2);
    const i = this.length++;
    for (const k of Object.keys(this.schema)) this.put(k, i, (row as any)[k]);
    return i;
  }
  private put(k: string, i: number, v: unknown) {
    const kind = this.schema[k].kind;
    const c = this.cols[k];
    const sv = store(kind, v);
    (c as any)[i] = kind === "bool" ? (sv ? 1 : 0) : sv;
  }
  private at(k: string, i: number): unknown {
    const v = (this.cols[k] as any)[i];
    return this.schema[k].kind === "bool" ? v === 1 : v;
  }
  /** A field's values, as stored (a typed array's live window, or the strings). */
  column<K extends keyof S & string>(k: K): Column {
    const c = this.cols[k];
    return Array.isArray(c) ? c.slice(0, this.length) : (c as any).subarray(0, this.length);
  }
  /** One row as a plain object. */
  row(i: number): Values<S> {
    const o: any = {};
    for (const k of Object.keys(this.schema)) o[k] = this.at(k, i);
    return o;
  }
  /** Each row as a view for one step of the loop; `write` lets the loop change it in place. */
  *rows(mode: "read" | "write" = "read"): Generator<Values<S>> {
    for (let i = 0; i < this.length; i++) {
      const born = ++generation;
      yield view(this.schema, (k) => this.at(k, i), mode === "write" ? (k, v) => this.put(k, i, v) : null, () => generation, born, this.name) as Values<S>;
      generation++;
    }
  }
  /** The rows that pass, as a new table. */
  where(pass: (r: Readonly<Values<S>>) => boolean): Table<S> {
    const t = new Table(this.schema, this.name);
    for (let i = 0; i < this.length; i++) {
      const r = this.row(i);
      if (pass(r)) t.push(r);
    }
    return t;
  }
  toArray(): Values<S>[] {
    return Array.from({ length: this.length }, (_, i) => this.row(i));
  }
}

/** A table of a schema, from rows if given. */
export function table<S extends Schema>(schema: S, rows: Values<S>[] = [], name = "table"): Table<S> {
  const t = new Table(schema, name);
  for (const r of rows) t.push(r);
  return t;
}

/** Rows of `a` and `b` whose keys are equal (a hash join), as pairs of plain rows. */
export function join<A extends Schema, B extends Schema>(a: Table<A>, ka: keyof A & string, b: Table<B>, kb: keyof B & string): [Values<A>, Values<B>][] {
  const index = new Map<unknown, number[]>();
  const col = b.column(kb);
  for (let i = 0; i < b.length; i++) {
    const key = (col as any)[i];
    if (!index.has(key)) index.set(key, []);
    index.get(key)!.push(i);
  }
  const out: [Values<A>, Values<B>][] = [];
  const ca = a.column(ka);
  for (let i = 0; i < a.length; i++) {
    for (const j of index.get((ca as any)[i]) ?? []) out.push([a.row(i), b.row(j)]);
  }
  return out;
}
