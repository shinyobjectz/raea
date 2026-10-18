// Debug overlays (react/docs/playtest.md, "views"): drawings a game or piece keeps beside its picture (hitboxes, its
// state as text, a path) that show only in a render asked for them. A playtest view renders the same frame clean for a
// taste question and with overlays for a diagnosis; the overlays are never in the clean render, nor in live play.
// The render asks by setting globalThis.__RAEA_OVERLAYS__ (names, or "*" for all) before the bundle runs.
import type { ReactNode } from "react";

/** The overlays this render was asked for ([] in a clean render and in live play). */
export function overlays(): string[] {
  const o = (globalThis as { __RAEA_OVERLAYS__?: unknown }).__RAEA_OVERLAYS__;
  return Array.isArray(o) ? o.filter((x): x is string => typeof x === "string") : [];
}

/** Its children only in a render asked for the overlay `name`. */
export function Overlay({ name, children }: { name: string; children?: ReactNode }) {
  const on = overlays();
  return on.includes(name) || on.includes("*") ? (children ?? null) : null;
}
