// @raea/motion/ecs: an entity-component world held by the engine (Bevy's own ECS: components by name in its archetype
// tables, its queries, its parent/child relationship; resources and per-frame events beside), with TypeScript systems
// stepping it frame by frame. `world.at(frame)` is the world after frames 0..frame-1 have run, the
// same whenever it's asked for (a frame of a video is a function of the frame, as rrl's `next:` rules make it). Under
// plain bun the tables are TypeScript's own, with the same behaviour, so tests run anywhere.

import { hasNative, native } from "./native";

export type Value = number | string | boolean;
export type Cols = Record<string, Value>;
export type Op = "==" | "!=" | "<" | "<=" | ">" | ">=";
export type Where = [string, Op, Value][];
export interface Row extends Cols {
  id: number;
  kind: string;
}

/** A system: what happens to the world on each frame. */
export type System = (world: World, frame: number) => void;

/** An entity's components by name; each a JSON value (usually an object of fields). */
export type Components = Record<string, unknown>;
/** What `find` asks for: every `with` component, none of the `without`, and conditions on "Comp.field". */
export interface Find {
  with?: string[];
  without?: string[];
  where?: Where;
}

interface Store {
  spawn(kind: string, cols: Cols): number;
  spawnComponents(comps: Components, parent?: number): number;
  insert(id: number, comps: Components): void;
  remove(id: number, names: string[]): void;
  components(id: number): Components;
  find(f: Find): number[];
  setParent(id: number, parent: number | null): void;
  parent(id: number): number | null;
  children(id: number): number[];
  resource(name: string, value?: unknown): unknown;
  send(name: string, value: unknown): void;
  events(name: string): unknown[];
  clearEvents(): void;
  set(id: number, cols: Cols): void;
  get(id: number): Row;
  despawn(id: number): void;
  query(kind: string, where: Where): number[];
  column(kind: string, col: string): (number | null)[];
  count(kind: string): number;
  rows(): Row[];
  clone(): Store;
  free(): void;
}

class NativeStore implements Store {
  constructor(readonly world: number) {}
  static create(): NativeStore {
    return new NativeStore(native("ecs.new", {}).world);
  }
  spawn(kind: string, cols: Cols) {
    return native("ecs.spawn", { world: this.world, kind, cols }).id;
  }
  spawnComponents(components: Components, parent?: number) {
    return native("ecs.spawn", parent === undefined ? { world: this.world, components } : { world: this.world, components, parent }).id;
  }
  insert(id: number, components: Components) {
    native("ecs.insert", { world: this.world, id, components });
  }
  remove(id: number, names: string[]) {
    native("ecs.remove", { world: this.world, id, names });
  }
  components(id: number) {
    return native("ecs.components", { world: this.world, id }) as Components;
  }
  find(f: Find) {
    return native("ecs.find", { world: this.world, with: f.with ?? [], without: f.without ?? [], where: f.where ?? [] }).ids;
  }
  setParent(id: number, parent: number | null) {
    native("ecs.parent", { world: this.world, id, parent });
  }
  parent(id: number) {
    return native("ecs.parent", { world: this.world, id }) as number | null;
  }
  children(id: number) {
    return native("ecs.children", { world: this.world, id });
  }
  resource(name: string, value?: unknown) {
    return native("ecs.resource", value === undefined ? { world: this.world, name } : { world: this.world, name, value });
  }
  send(name: string, value: unknown) {
    native("ecs.send", { world: this.world, name, value });
  }
  events(name: string) {
    return native("ecs.events", { world: this.world, name });
  }
  clearEvents() {
    native("ecs.clear_events", { world: this.world });
  }
  set(id: number, cols: Cols) {
    native("ecs.set", { world: this.world, id, cols });
  }
  get(id: number) {
    return native("ecs.get", { world: this.world, id });
  }
  despawn(id: number) {
    native("ecs.despawn", { world: this.world, id });
  }
  query(kind: string, where: Where) {
    return native("ecs.query", { world: this.world, kind, where }).ids;
  }
  column(kind: string, col: string) {
    return native("ecs.column", { world: this.world, kind, col });
  }
  count(kind: string) {
    return native("ecs.count", { world: this.world, kind });
  }
  rows() {
    return native("ecs.rows", { world: this.world });
  }
  clone() {
    return new NativeStore(native("ecs.clone", { world: this.world }).world);
  }
  free() {
    native("ecs.free", { world: this.world });
  }
}

class JsStore implements Store {
  private next = 0;
  private ents = new Map<number, { kind?: string; comps: Map<string, unknown>; parent: number | null }>();
  private res = new Map<string, unknown>();
  private ev = new Map<string, unknown[]>();
  private must(id: number, what: string) {
    const e = this.ents.get(id);
    if (!e) throw new Error(`${what}: no entity ${id}`);
    return e;
  }
  spawn(kind: string, cols: Cols) {
    const id = this.spawnComponents({ [kind]: { ...cols } });
    this.ents.get(id)!.kind = kind;
    return id;
  }
  spawnComponents(comps: Components, parent?: number) {
    const id = ++this.next;
    if (parent !== undefined) this.must(parent, "spawn");
    this.ents.set(id, { comps: new Map(Object.entries(comps).map(([k, v]) => [k, structuredCopy(v)])), parent: parent ?? null });
    return id;
  }
  set(id: number, cols: Cols) {
    const e = this.must(id, "ecs.set");
    const k = e.kind ?? "";
    e.comps.set(k, { ...(e.comps.get(k) as Cols), ...cols });
  }
  get(id: number) {
    const e = this.must(id, "ecs.get");
    return { ...((e.comps.get(e.kind ?? "") as Cols) ?? {}), id, kind: e.kind ?? "" } as Row;
  }
  despawn(id: number) {
    this.must(id, "ecs.despawn");
    const gone = [id];
    for (let i = 0; i < gone.length; i++) for (const [cid, c] of this.ents) if (c.parent === gone[i]) gone.push(cid);
    for (const g of gone) this.ents.delete(g);
  }
  insert(id: number, comps: Components) {
    const e = this.must(id, "insert");
    for (const [k, v] of Object.entries(comps)) e.comps.set(k, structuredCopy(v));
  }
  remove(id: number, names: string[]) {
    const e = this.must(id, "remove");
    for (const n of names) e.comps.delete(n);
  }
  components(id: number) {
    return Object.fromEntries([...this.must(id, "components").comps].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)).map(([k, v]) => [k, structuredCopy(v)]));
  }
  find(f: Find) {
    const out: number[] = [];
    const dflt = f.with?.[0];
    for (const [id, e] of this.ents) {
      if (!(f.with ?? []).every((n) => e.comps.has(n)) || (f.without ?? []).some((n) => e.comps.has(n))) continue;
      if ((f.where ?? []).every(([col, op, v]) => {
        const [c, field] = col.includes(".") ? col.split(".", 2) : [dflt ?? "", col];
        const comp = e.comps.get(c) as Record<string, Value> | undefined;
        return test(comp?.[field], op, v);
      })) out.push(id);
    }
    return out.sort((a, b) => a - b);
  }
  query(kind: string, where: Where) {
    return this.find({ with: [kind], where }).filter((id) => this.ents.get(id)!.kind === kind);
  }
  column(kind: string, col: string) {
    return this.query(kind, []).map((id) => {
      const v = (this.ents.get(id)!.comps.get(kind) as Cols)[col];
      return typeof v === "number" ? v : null;
    });
  }
  count(kind: string) {
    return this.query(kind, []).length;
  }
  rows() {
    const ids = [...this.ents].filter(([, e]) => e.kind !== undefined).map(([id, e]) => [e.kind!, id] as const);
    ids.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] - b[1]));
    return ids.map(([, id]) => this.get(id));
  }
  setParent(id: number, parent: number | null) {
    const e = this.must(id, "parent");
    if (parent !== null) this.must(parent, "parent");
    e.parent = parent;
  }
  parent(id: number) {
    return this.must(id, "parent").parent;
  }
  children(id: number) {
    return [...this.ents].filter(([, e]) => e.parent === id).map(([cid]) => cid).sort((a, b) => a - b);
  }
  resource(name: string, value?: unknown) {
    if (value !== undefined) this.res.set(name, structuredCopy(value));
    return this.res.has(name) ? structuredCopy(this.res.get(name)) : null;
  }
  send(name: string, value: unknown) {
    const l = this.ev.get(name) ?? [];
    l.push(structuredCopy(value));
    this.ev.set(name, l);
  }
  events(name: string) {
    return (this.ev.get(name) ?? []).map(structuredCopy);
  }
  clearEvents() {
    this.ev.clear();
  }
  clone() {
    const c = new JsStore();
    c.next = this.next;
    for (const [k, e] of this.ents) c.ents.set(k, { kind: e.kind, comps: new Map([...e.comps].map(([n, v]) => [n, structuredCopy(v)])), parent: e.parent });
    for (const [k, v] of this.res) c.res.set(k, structuredCopy(v));
    for (const [k, v] of this.ev) c.ev.set(k, v.map(structuredCopy));
    return c;
  }
  free() {
    this.ents.clear();
  }
}

/** A deep copy of plain data (the engine's components are JSON; TypeScript's tables keep the same rule). */
function structuredCopy<T>(v: T): T {
  return v === undefined ? v : (JSON.parse(JSON.stringify(v)) as T);
}

function test(x: Value | undefined, op: Op, v: Value): boolean {
  const y = typeof x === "boolean" ? (x ? 1 : 0) : x;
  const w = typeof v === "boolean" ? (v ? 1 : 0) : v;
  switch (op) {
    case "==":
      return y === w;
    case "!=":
      return y !== w;
    case "<":
      return typeof y === "number" && typeof w === "number" && y < w;
    case "<=":
      return typeof y === "number" && typeof w === "number" && y <= w;
    case ">":
      return typeof y === "number" && typeof w === "number" && y > w;
    case ">=":
      return typeof y === "number" && typeof w === "number" && y >= w;
  }
}

/** A world of entities: rows in tables by kind. */
export class World {
  private store: Store;
  private systems: System[] = [];
  private initial: Store | null = null;
  private cache: { frame: number; store: Store } | null = null;
  /** whether the tables are the engine's (native) or TypeScript's */
  readonly native: boolean;

  constructor(store?: Store) {
    this.native = store ? store instanceof NativeStore : hasNative();
    this.store = store ?? (this.native ? NativeStore.create() : new JsStore());
  }

  /** A new entity of `kind` with its columns; its id. */
  spawn(kind: string, cols: Cols = {}): number {
    this.dirty();
    return this.store.spawn(kind, cols);
  }
  /** Columns set on an entity. */
  set(id: number, cols: Cols): void {
    this.store.set(id, cols);
  }
  /** An entity's row. */
  get(id: number): Row {
    return this.store.get(id);
  }
  despawn(id: number): void {
    this.store.despawn(id);
  }
  /** The ids of `kind`'s entities whose columns pass every condition, filtered by the engine. */
  query(kind: string, where: Where = []): number[] {
    return this.store.query(kind, where);
  }
  /** One numeric column of `kind`, in id order (null where unset). */
  column(kind: string, col: string): (number | null)[] {
    return this.store.column(kind, col);
  }
  count(kind: string): number {
    return this.store.count(kind);
  }
  /** Every row, by kind then id. */
  rows(): Row[] {
    return this.store.rows();
  }
  /** A new entity with components ({Position: {x, y}, Ball: {}}), optionally a child of `parent`. */
  add(components: Components, opts: { parent?: number } = {}): number {
    this.dirty();
    return this.store.spawnComponents(components, opts.parent);
  }
  /** Components added to (or replacing those on) an entity. */
  insert(id: number, components: Components): void {
    this.store.insert(id, components);
  }
  /** Components taken off an entity. */
  remove(id: number, ...names: string[]): void {
    this.store.remove(id, names);
  }
  /** An entity's components by name. */
  components(id: number): Components {
    return this.store.components(id);
  }
  /** One component of an entity (undefined when it hasn't it). */
  component<T = Record<string, Value>>(id: number, name: string): T | undefined {
    return this.store.components(id)[name] as T | undefined;
  }
  /** Entities with every `with` component and no `without` one, passing every condition ("Comp.field", or a field of
   * the first `with`), in id order: Bevy's query in the engine. */
  find(f: Find): number[] {
    return this.store.find(f);
  }
  /** An entity's parent (despawning a parent despawns its children). */
  setParent(id: number, parent: number | null): void {
    this.store.setParent(id, parent);
  }
  parent(id: number): number | null {
    return this.store.parent(id);
  }
  children(id: number): number[] {
    return this.store.children(id);
  }
  /** A resource: one value for the world (set it with a value; null when unset). */
  resource<T = unknown>(name: string, value?: T): T | null {
    return this.store.resource(name, value) as T | null;
  }
  /** An event for the systems after this one, this frame (each frame starts with none). */
  send(name: string, value: unknown = null): void {
    this.store.send(name, value);
  }
  /** This frame's events of a name, in the order sent. */
  events<T = unknown>(name: string): T[] {
    return this.store.events(name) as T[];
  }
  /** What draws, straight from the engine's world: Mesh + Transform entities (world space, parents composed) and
   * lights. Only with native tables (`<Entities world={w.at(frame)} />` hands this to the renderer). */
  scene(): unknown[] {
    if (!(this.store instanceof NativeStore)) throw new Error("scene() needs the engine's tables");
    return native("ecs.scene", { world: this.store.world });
  }
  /** The engine's id of this world's tables (for the renderer), or null under plain bun. */
  get handle(): number | null {
    return this.store instanceof NativeStore ? this.store.world : null;
  }
  /** A system run on every frame, after the ones before it. */
  system(fn: System): this {
    this.systems.push(fn);
    this.dirty();
    return this;
  }
  /** Run the systems once, for `frame`. */
  step(frame: number): void {
    this.store.clearEvents();
    for (const s of this.systems) s(this, frame);
  }
  /** The world after frames 0..frame-1 have run (a copy; this world as built is the state before frame 0). */
  at(frame: number): World {
    if (!Number.isInteger(frame) || frame < 0) throw new RangeError(`World.at: a frame number from 0, got ${frame}`);
    if (!this.initial) this.initial = this.store.clone();
    let from = 0;
    let store: Store;
    if (this.cache && this.cache.frame <= frame) {
      from = this.cache.frame;
      store = this.cache.store.clone();
    } else {
      store = this.initial.clone();
    }
    const w = new World(store);
    w.systems = this.systems;
    for (let f = from; f < frame; f++) w.step(f);
    if (this.cache) this.cache.store.free();
    this.cache = { frame, store: store.clone() };
    return w;
  }
  private dirty() {
    if (this.initial) this.initial.free();
    if (this.cache) this.cache.store.free();
    this.initial = null;
    this.cache = null;
  }
}

/** A new world: native tables in the engine, TypeScript's under plain bun (or with `{ native: false }`). */
export function createWorld(opts: { native?: boolean } = {}): World {
  if (opts.native === false) return new World(new JsStore());
  if (opts.native === true && !hasNative()) throw new Error("createWorld({ native: true }) needs the raea engine");
  return new World();
}
