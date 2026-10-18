// The engine's native calls (`__raea_call(name, json) -> json`, raea-native's registry): present when this code runs
// inside raea's V8 (raea-motion render, run or test, the apps, the iPhone); absent under plain bun, where the callers
// fall back or say what's needed. Names, arguments and answers are typed by native-calls.ts, generated from the registry.

import type { NativeCalls, NativeName } from "./native-calls";
export type { NativeCalls, NativeName } from "./native-calls";

type NativeCall = (name: string, json: string) => string | undefined;

function bridge(): NativeCall | null {
  const g = globalThis as { __raea_call?: NativeCall };
  return typeof g.__raea_call === "function" ? g.__raea_call : null;
}

/** Whether the engine's native calls are here. */
export function hasNative(): boolean {
  return bridge() !== null;
}

/** One native call; its answer parsed. Throws (with the engine's message) when the call fails. */
export function native<K extends NativeName>(name: K, args: NativeCalls[K]["args"]): NativeCalls[K]["answer"] {
  const call = bridge();
  if (!call) throw new Error(`${name} needs the raea engine (raea-motion render or run); it isn't available under plain bun`);
  const out = call(name, JSON.stringify(args));
  return (out === undefined || out === "" ? undefined : JSON.parse(out)) as NativeCalls[K]["answer"];
}

/** Print a line from a headless run (raea-motion run). */
export function print(...parts: unknown[]): void {
  const text = parts.map((p) => (typeof p === "string" ? p : JSON.stringify(p))).join(" ");
  if (hasNative()) native("print", { text });
  else console.log(text);
}
