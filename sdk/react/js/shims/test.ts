// The test API on raea's V8: what `bun:test` (and the vitest names the same suites use) offers, enough to run the
// conformance suite and vendored packages' own tests inside the engine (raea-motion test). Tests register as files load;
// raea-motion's test entry then calls __raeaTestRun(), which runs them in order and reports each line through print.

type Fn = () => unknown | Promise<unknown>;
interface Case {
  name: string;
  fn: Fn;
  skip: boolean;
  only: boolean;
  timeout?: number;
}
interface Suite {
  name: string;
  parent: Suite | null;
  cases: (Case | Suite)[];
  skip: boolean;
  beforeAll: Fn[];
  afterAll: Fn[];
  beforeEach: Fn[];
  afterEach: Fn[];
}
const root: Suite = { name: "", parent: null, cases: [], skip: false, beforeAll: [], afterAll: [], beforeEach: [], afterEach: [] };
let current = root;
let anyOnly = false;

function addCase(name: string, fn: Fn | undefined, skip = false, only = false, timeout?: number) {
  if (only) anyOnly = true;
  current.cases.push({ name, fn: fn ?? (() => {}), skip: skip || !fn, only, timeout });
}
function addSuite(name: string, body: () => void, skip = false) {
  const s: Suite = { name, parent: current, cases: [], skip, beforeAll: [], afterAll: [], beforeEach: [], afterEach: [] };
  current.cases.push(s);
  const prev = current;
  current = s;
  try {
    body();
  } finally {
    current = prev;
  }
}

type TestFn = ((name: string, fn?: Fn, timeout?: number | { timeout?: number }) => void) & {
  skip: (name: string, fn?: Fn) => void;
  only: (name: string, fn?: Fn) => void;
  todo: (name: string, fn?: Fn) => void;
  skipIf: (cond: unknown) => (name: string, fn?: Fn) => void;
  if: (cond: unknown) => (name: string, fn?: Fn) => void;
  each: <T>(rows: T[]) => (name: string, fn: (...args: any[]) => unknown) => void;
};
const tmo = (t: number | { timeout?: number } | undefined) => (typeof t === "number" ? t : t?.timeout);
function format(name: string, row: unknown, i: number): string {
  const args = Array.isArray(row) ? row : [row];
  let k = 0;
  return name.replace(/%[sdifjo#%]/g, (m) => (m === "%%" ? "%" : m === "%#" ? String(i) : m === "%j" || m === "%o" ? JSON.stringify(args[k++]) : String(args[k++])));
}
export const test: TestFn = Object.assign((name: string, fn?: Fn, timeout?: number | { timeout?: number }) => addCase(name, fn, false, false, tmo(timeout)), {
  skip: (name: string, fn?: Fn) => addCase(name, fn, true),
  only: (name: string, fn?: Fn) => addCase(name, fn, false, true),
  todo: (name: string, fn?: Fn) => addCase(name, fn, true),
  skipIf: (cond: unknown) => (name: string, fn?: Fn) => addCase(name, fn, !!cond),
  if: (cond: unknown) => (name: string, fn?: Fn) => addCase(name, fn, !cond),
  each:
    <T,>(rows: T[]) =>
    (name: string, fn: (...args: any[]) => unknown) =>
      rows.forEach((row, i) => addCase(format(name, row, i), () => (Array.isArray(row) ? fn(...row) : fn(row)))),
});
export const it = test;
type DescribeFn = ((name: string, body: () => void) => void) & {
  skip: (name: string, body: () => void) => void;
  only: (name: string, body: () => void) => void;
  skipIf: (cond: unknown) => (name: string, body: () => void) => void;
  if: (cond: unknown) => (name: string, body: () => void) => void;
  each: <T>(rows: T[]) => (name: string, body: (...args: any[]) => void) => void;
};
export const describe: DescribeFn = Object.assign((name: string, body: () => void) => addSuite(name, body), {
  skip: (name: string, body: () => void) => addSuite(name, body, true),
  only: (name: string, body: () => void) => addSuite(name, body),
  skipIf: (cond: unknown) => (name: string, body: () => void) => addSuite(name, body, !!cond),
  if: (cond: unknown) => (name: string, body: () => void) => addSuite(name, body, !cond),
  each:
    <T,>(rows: T[]) =>
    (name: string, body: (...args: any[]) => void) =>
      rows.forEach((row, i) => addSuite(format(name, row, i), () => (Array.isArray(row) ? body(...row) : body(row)))),
});
export const beforeAll = (fn: Fn) => current.beforeAll.push(fn);
export const afterAll = (fn: Fn) => current.afterAll.push(fn);
export const beforeEach = (fn: Fn) => current.beforeEach.push(fn);
export const afterEach = (fn: Fn) => current.afterEach.push(fn);

// ---- expect ---------------------------------------------------------------------------------------------------------
class Asym {
  constructor(
    readonly test: (v: unknown) => boolean,
    readonly label: string,
  ) {}
}
function show(v: unknown): string {
  if (v instanceof Asym) return v.label;
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "bigint") return `${v}n`;
  if (typeof v === "function") return `[Function ${v.name || "anonymous"}]`;
  if (v instanceof Error) return `${v.name}: ${v.message}`;
  if (v instanceof Map) return `Map(${show([...v])})`;
  if (v instanceof Set) return `Set(${show([...v])})`;
  try {
    const s = JSON.stringify(v, (_k, x) => (typeof x === "bigint" ? `${x}n` : x === undefined ? "<undefined>" : x));
    return s === undefined ? String(v) : s.length > 300 ? s.slice(0, 300) + "..." : s;
  } catch {
    return String(v);
  }
}
function equals(a: unknown, b: unknown, strict: boolean, seen = new Map<unknown, unknown>()): boolean {
  if (b instanceof Asym) return b.test(a);
  if (a instanceof Asym) return a.test(b);
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (seen.get(a) === b) return true;
  seen.set(a, b);
  if (strict && Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  if (a instanceof Date && b instanceof Date) return a.getTime() === b.getTime();
  if (a instanceof RegExp && b instanceof RegExp) return String(a) === String(b);
  if (a instanceof Map && b instanceof Map) {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) if (!b.has(k) || !equals(v, b.get(k), strict, seen)) return false;
    return true;
  }
  if (a instanceof Set && b instanceof Set) {
    if (a.size !== b.size) return false;
    for (const v of a) if (![...b].some((w) => equals(v, w, strict, seen))) return false;
    return true;
  }
  if (ArrayBuffer.isView(a) && ArrayBuffer.isView(b)) {
    const x = a as unknown as ArrayLike<number>, y = b as unknown as ArrayLike<number>;
    if (x.length !== y.length || a.constructor !== b.constructor) return false;
    for (let i = 0; i < x.length; i++) if (!Object.is(x[i], y[i])) return false;
    return true;
  }
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a) && Array.isArray(b) && a.length !== b.length) return false;
  const keys = (o: object) => Object.keys(o).filter((k) => strict || (o as Record<string, unknown>)[k] !== undefined);
  const ka = keys(a), kb = keys(b);
  if (ka.length !== kb.length) return false;
  for (const k of ka) {
    if (!Object.prototype.hasOwnProperty.call(b, k) && !(b instanceof Asym)) return false;
    if (!equals((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], strict, seen)) return false;
  }
  return true;
}
function matchesObject(a: unknown, b: unknown): boolean {
  if (b instanceof Asym) return b.test(a);
  if (typeof b !== "object" || b === null) return equals(a, b, false);
  if (typeof a !== "object" || a === null) return false;
  if (Array.isArray(b)) return Array.isArray(a) && a.length === b.length && b.every((x, i) => matchesObject(a[i], x));
  return Object.keys(b).every((k) => matchesObject((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

interface MockFn {
  (...args: unknown[]): unknown;
  mock: { calls: unknown[][]; results: { type: string; value: unknown }[] };
  mockImplementation(f: (...a: unknown[]) => unknown): MockFn;
  mockReturnValue(v: unknown): MockFn;
  mockResolvedValue(v: unknown): MockFn;
  mockClear(): MockFn;
  mockReset(): MockFn;
  _isMock: true;
}
function fn(impl?: (...a: unknown[]) => unknown): MockFn {
  let f = impl;
  const m = function (this: unknown, ...args: unknown[]) {
    m.mock.calls.push(args);
    try {
      const value = f ? f.apply(this, args) : undefined;
      m.mock.results.push({ type: "return", value });
      return value;
    } catch (e) {
      m.mock.results.push({ type: "throw", value: e });
      throw e;
    }
  } as MockFn;
  m.mock = { calls: [], results: [] };
  m._isMock = true;
  m.mockImplementation = (g) => ((f = g), m);
  m.mockReturnValue = (v) => ((f = () => v), m);
  m.mockResolvedValue = (v) => ((f = () => Promise.resolve(v)), m);
  m.mockClear = () => ((m.mock.calls = []), (m.mock.results = []), m);
  m.mockReset = () => ((f = undefined), m.mockClear());
  return m;
}
export const mock = fn;
const stubbed = new Map<string, { had: boolean; value: unknown }>();
export const vi = {
  fn,
  spyOn<T extends object>(obj: T, key: keyof T) {
    const orig = obj[key] as unknown as (...a: unknown[]) => unknown;
    const m = fn((...a: unknown[]) => orig.apply(obj, a));
    (obj as Record<string, unknown>)[key as string] = m;
    (m as unknown as { mockRestore: () => void }).mockRestore = () => ((obj as Record<string, unknown>)[key as string] = orig);
    return m;
  },
  restoreAllMocks() {},
  stubGlobal(name: string, value: unknown) {
    const g = globalThis as Record<string, unknown>;
    if (!stubbed.has(name)) stubbed.set(name, { had: name in g, value: g[name] });
    g[name] = value;
  },
  unstubAllGlobals() {
    const g = globalThis as Record<string, unknown>;
    for (const [k, v] of stubbed) {
      if (v.had) g[k] = v.value;
      else delete g[k];
    }
    stubbed.clear();
  },
  mocked<T>(x: T): T {
    return x;
  },
  clearAllMocks() {},
  useFakeTimers() {},
  useRealTimers() {},
};
export const jest = vi;
export const spyOn = vi.spyOn;

class Failure extends Error {}

function makeMatchers(actual: unknown, negate: boolean, label: string) {
  const check = (pass: boolean, msg: () => string) => {
    if (pass === negate) throw new Failure(`expect(${label})${negate ? ".not" : ""}: ${msg()}`);
  };
  const thrown = (): { threw: boolean; error: unknown } => {
    if (typeof actual !== "function") return { threw: false, error: undefined };
    try {
      (actual as () => unknown)();
      return { threw: false, error: undefined };
    } catch (e) {
      return { threw: true, error: e };
    }
  };
  const errMatches = (e: unknown, want: unknown): boolean => {
    if (want === undefined) return true;
    const msg = e instanceof Error ? e.message : String(e);
    if (typeof want === "string") return msg.includes(want);
    if (want instanceof RegExp) return want.test(msg);
    if (typeof want === "function") return e instanceof (want as new () => unknown);
    if (want instanceof Error) return msg === want.message;
    if (typeof want === "object" && want !== null) return matchesObject(e, want);
    return false;
  };
  const calls = () => {
    const m = actual as MockFn;
    if (!m || !m._isMock) throw new Failure(`expect(${label}): not a mock function`);
    return m.mock.calls;
  };
  const m = {
    toBe: (b: unknown) => check(Object.is(actual, b), () => `expected ${show(actual)} to be ${show(b)}`),
    toEqual: (b: unknown) => check(equals(actual, b, false), () => `expected ${show(actual)} to equal ${show(b)}`),
    toStrictEqual: (b: unknown) => check(equals(actual, b, true), () => `expected ${show(actual)} to strictly equal ${show(b)}`),
    toMatchObject: (b: unknown) => check(matchesObject(actual, b), () => `expected ${show(actual)} to match ${show(b)}`),
    toBeCloseTo: (b: number, digits = 2) => check(Math.abs((actual as number) - b) < 10 ** -digits / 2, () => `expected ${show(actual)} to be close to ${b} (${digits} digits)`),
    toBeGreaterThan: (b: number) => check((actual as number) > b, () => `expected ${show(actual)} > ${b}`),
    toBeGreaterThanOrEqual: (b: number) => check((actual as number) >= b, () => `expected ${show(actual)} >= ${b}`),
    toBeLessThan: (b: number) => check((actual as number) < b, () => `expected ${show(actual)} < ${b}`),
    toBeLessThanOrEqual: (b: number) => check((actual as number) <= b, () => `expected ${show(actual)} <= ${b}`),
    toBeNull: () => check(actual === null, () => `expected ${show(actual)} to be null`),
    toBeUndefined: () => check(actual === undefined, () => `expected ${show(actual)} to be undefined`),
    toBeDefined: () => check(actual !== undefined, () => `expected a value, got undefined`),
    toBeNaN: () => check(Number.isNaN(actual), () => `expected ${show(actual)} to be NaN`),
    toBeTruthy: () => check(!!actual, () => `expected ${show(actual)} to be truthy`),
    toBeFalsy: () => check(!actual, () => `expected ${show(actual)} to be falsy`),
    toBeInstanceOf: (c: new (...a: any[]) => unknown) => check(actual instanceof c, () => `expected ${show(actual)} to be an instance of ${c.name}`),
    toBeTypeOf: (t: string) => check(typeof actual === t, () => `expected typeof ${show(actual)} to be ${t}`),
    toContain: (x: unknown) =>
      check(typeof actual === "string" ? actual.includes(x as string) : Array.from(actual as Iterable<unknown>).includes(x), () => `expected ${show(actual)} to contain ${show(x)}`),
    toContainEqual: (x: unknown) => check(Array.from(actual as Iterable<unknown>).some((y) => equals(y, x, false)), () => `expected ${show(actual)} to contain ${show(x)}`),
    toHaveLength: (n: number) => check((actual as { length: number }).length === n, () => `expected length ${n}, got ${(actual as { length: number })?.length}`),
    toHaveProperty: (path: string | string[], value?: unknown) => {
      const keys = Array.isArray(path) ? path : path.split(".");
      let o: unknown = actual;
      let has = true;
      for (const k of keys) {
        if (o !== null && o !== undefined && k in Object(o)) o = (o as Record<string, unknown>)[k];
        else {
          has = false;
          break;
        }
      }
      check(has && (value === undefined || equals(o, value, false)), () => `expected ${show(actual)} to have ${keys.join(".")}${value === undefined ? "" : ` = ${show(value)}`}`);
    },
    toMatch: (re: RegExp | string) => check(typeof re === "string" ? String(actual).includes(re) : re.test(String(actual)), () => `expected ${show(actual)} to match ${String(re)}`),
    toThrow: (want?: unknown) => {
      const t = thrown();
      check(t.threw && errMatches(t.error, want), () => (t.threw ? `threw ${show(t.error)}, not matching ${show(want)}` : `expected it to throw`));
    },
    toThrowError: (want?: unknown) => m.toThrow(want),
    toHaveBeenCalled: () => check(calls().length > 0, () => `expected a call`),
    toHaveBeenCalledTimes: (n: number) => check(calls().length === n, () => `expected ${n} calls, got ${calls().length}`),
    toHaveBeenCalledWith: (...args: unknown[]) => check(calls().some((c) => equals(c, args, false)), () => `expected a call with ${show(args)}, got ${show(calls())}`),
    toHaveBeenLastCalledWith: (...args: unknown[]) => check(equals(calls()[calls().length - 1], args, false), () => `expected the last call with ${show(args)}`),
  };
  return m;
}
type Matchers = ReturnType<typeof makeMatchers>;
type AsyncMatchers = { [K in keyof Matchers]: (...a: Parameters<Matchers[K]>) => Promise<void> };
interface Expectation extends Matchers {
  not: Matchers;
  resolves: AsyncMatchers & { not: AsyncMatchers };
  rejects: AsyncMatchers & { not: AsyncMatchers };
}
function settled(p: unknown, want: "resolves" | "rejects", negate: boolean): AsyncMatchers {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(makeMatchers(undefined, false, ""))) {
    out[k] = async (...args: unknown[]) => {
      let value: unknown;
      try {
        value = await p;
        if (want === "rejects") throw new Failure(`expected the promise to reject, it resolved ${show(value)}`);
      } catch (e) {
        if (e instanceof Failure) throw e;
        if (want === "resolves") throw new Failure(`expected the promise to resolve, it rejected ${show(e)}`);
        value = e;
      }
      const target = want === "rejects" && (k === "toThrow" || k === "toThrowError") ? () => { throw value; } : value;
      (makeMatchers(target, negate, want) as Record<string, (...a: unknown[]) => void>)[k](...args);
    };
  }
  return out as AsyncMatchers;
}
export const expect = Object.assign(
  (actual: unknown): Expectation => {
    const label = typeof actual === "function" ? "fn" : show(actual).slice(0, 60);
    return Object.assign(makeMatchers(actual, false, label), {
      not: makeMatchers(actual, true, label),
      resolves: Object.assign(settled(actual, "resolves", false), { not: settled(actual, "resolves", true) }),
      rejects: Object.assign(settled(actual, "rejects", false), { not: settled(actual, "rejects", true) }),
    });
  },
  {
    any: (c: unknown) => new Asym((v) => (c === Number ? typeof v === "number" : c === String ? typeof v === "string" : c === Boolean ? typeof v === "boolean" : c === Function ? typeof v === "function" : v instanceof (c as new () => unknown)), `Any<${(c as { name?: string }).name}>`),
    anything: () => new Asym((v) => v !== null && v !== undefined, "Anything"),
    stringContaining: (s: string) => new Asym((v) => typeof v === "string" && v.includes(s), `StringContaining ${s}`),
    stringMatching: (re: RegExp | string) => new Asym((v) => typeof v === "string" && (typeof re === "string" ? v.includes(re) : re.test(v)), `StringMatching ${re}`),
    objectContaining: (o: object) => new Asym((v) => matchesObject(v, o), `ObjectContaining ${show(o)}`),
    arrayContaining: (a: unknown[]) => new Asym((v) => Array.isArray(v) && a.every((x) => v.some((y) => equals(y, x, false))), `ArrayContaining ${show(a)}`),
    closeTo: (n: number, digits = 2) => new Asym((v) => typeof v === "number" && Math.abs(v - n) < 10 ** -digits / 2, `CloseTo ${n}`),
    assertions: (_n: number) => {},
    hasAssertions: () => {},
  },
);

// ---- the run --------------------------------------------------------------------------------------------------------
function withTimeout(p: unknown, ms: number, name: string): Promise<unknown> {
  if (!(p instanceof Promise)) return Promise.resolve(p);
  return new Promise((res, rej) => {
    const t = setTimeout(() => rej(new Error(`timed out after ${ms} ms`)), ms);
    p.then(
      (v) => (clearTimeout(t), res(v)),
      (e) => (clearTimeout(t), rej(e)),
    );
  });
}
export interface Result {
  pass: number;
  fail: number;
  skip: number;
  failures: string[];
}
export async function run(print: (line: string) => void, filter?: string, defaultTimeout = 5000): Promise<Result> {
  const r: Result = { pass: 0, fail: 0, skip: 0, failures: [] };
  const hasOnly = (s: Suite | Case): boolean => ("cases" in s ? s.cases.some(hasOnly) : s.only);
  async function suite(s: Suite, path: string[], before: Fn[], after: Fn[], skipped: boolean) {
    const sk = skipped || s.skip;
    const runAll = async (fs: Fn[]) => {
      for (const f of fs) await f();
    };
    if (!sk) await runAll(s.beforeAll);
    for (const c of s.cases) {
      if ("cases" in c) {
        await suite(c, [...path, c.name], [...before, ...s.beforeEach], [...s.afterEach, ...after], sk);
        continue;
      }
      const full = [...path, c.name].filter(Boolean).join(" > ");
      if (sk || c.skip || (anyOnly && !c.only) || (filter && !full.includes(filter))) {
        r.skip++;
        continue;
      }
      const t0 = Date.now();
      try {
        for (const f of [...before, ...s.beforeEach]) await f();
        await withTimeout(c.fn(), c.timeout ?? defaultTimeout, full);
        for (const f of [...s.afterEach, ...after]) await f();
        r.pass++;
        print(`PASS  ${full} (${Date.now() - t0} ms)`);
      } catch (e) {
        r.fail++;
        const msg = e instanceof Error ? (e instanceof Failure ? e.message : `${e.name}: ${e.message}\n${(e.stack ?? "").split("\n").slice(1, 4).join("\n")}`) : String(e);
        r.failures.push(`${full}: ${msg}`);
        print(`FAIL  ${full}: ${msg}`);
      }
    }
    if (!sk) await runAll(s.afterAll);
  }
  void hasOnly;
  await suite(root, [], [], [], false);
  print(`${r.pass} pass, ${r.fail} fail, ${r.skip} skipped`);
  return r;
}
