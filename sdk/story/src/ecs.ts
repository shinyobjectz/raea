// A small ECS for stories (docs/story.feature ST0b): entities are numbers, components are named plain values, resources
// are named values on the world, and systems are functions over the world. Plain data throughout, so a world prints as
// JSON, crosses into raea check, and feeds a model's training rows. It mirrors raea's Bevy world in shape (entities with
// components, systems that derive) without depending on it: the SDK stays TypeScript with no runtime dependencies.

export type Entity = number;
export type System = (w: World) => void;

export class World {
  private next = 0;
  private readonly comps = new Map<string, Map<Entity, unknown>>();
  private readonly res = new Map<string, unknown>();

  /** A new entity with these components. */
  spawn(components: Record<string, unknown> = {}): Entity {
    const e = this.next++;
    for (const [k, v] of Object.entries(components)) this.insert(e, k, v);
    return e;
  }

  insert<T>(e: Entity, name: string, value: T): void {
    let m = this.comps.get(name);
    if (!m) this.comps.set(name, (m = new Map()));
    m.set(e, value);
  }

  get<T>(e: Entity, name: string): T | undefined {
    return this.comps.get(name)?.get(e) as T | undefined;
  }

  /** A component the caller knows is there (a query found the entity by it). */
  must<T>(e: Entity, name: string): T {
    const v = this.get<T>(e, name);
    if (v === undefined) throw new Error(`entity ${e} has no ${name}`);
    return v;
  }

  has(e: Entity, name: string): boolean {
    return this.comps.get(name)?.has(e) ?? false;
  }

  /** Entities with every named component, in the order they were spawned. */
  query(...names: string[]): Entity[] {
    if (names.length === 0) return [];
    const [first, ...rest] = names.map((n) => this.comps.get(n) ?? new Map<Entity, unknown>());
    return [...first.keys()].filter((e) => rest.every((m) => m.has(e))).sort((a, b) => a - b);
  }

  resource<T>(name: string): T | undefined {
    return this.res.get(name) as T | undefined;
  }

  setResource<T>(name: string, value: T): void {
    this.res.set(name, value);
  }

  /** Run systems in order. */
  run(...systems: System[]): this {
    for (const s of systems) s(this);
    return this;
  }

  /** Every entity's components and every resource, as plain data (for printing, raea check and training rows). */
  toJSON(): { entities: Record<string, Record<string, unknown>>; resources: Record<string, unknown> } {
    const entities: Record<string, Record<string, unknown>> = {};
    for (const [name, m] of this.comps) for (const [e, v] of m) (entities[e] ??= {})[name] = v;
    return { entities, resources: Object.fromEntries(this.res) };
  }
}
