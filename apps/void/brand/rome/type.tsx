// The film's type. Cormorant Garamond, a high-contrast Garamond cut for display: names and dates in tracked capitals,
// what's said about them in italic, quotes large and light. Ivory on the picture's own dark, never on a card: each
// block sits in a soft pool of shadow (a radial fade, no edges) and its letters carry a shadow of their own, so it
// reads over white marble. Gold is for rules and numerals only.
//
// The scroll, abstracted: two gold hairlines part from the middle like a scroll's rods, and the words are there
// between them; they come together again as it goes. Letters arrive one by one, a few hundredths of a second apart.
import type { ReactNode } from "react";
import { clamp01, smooth } from "./time";

export const ivory = "#f2eadb", gold = "#d6b97e", ash = "#d2c7b4", blood = "#e8584a";
export const serif = "Cormorant Garamond";
/** the letters' own shadow: a wide soft dark and a tight one at the edges */
export const shade = "0 0 22px rgba(0,0,0,0.9), 0 0 6px rgba(0,0,0,0.75)";

/** rough set widths at 1 px type: Cormorant's capitals with their tracking, its italic lower case */
export const capsW = (s: string, size: number, track: number) => s.length * size * (0.64 + track);
export const italW = (s: string, size: number) => [...s].reduce((n, c) => n + (c >= "A" && c <= "Z" ? 0.62 : 0.4), 0) * size;
/** lines `s` wraps to in italic at `size` in a column `w` wide (greedy) */
export function wrapItal(s: string, size: number, w: number) {
  let lines = 1, run = 0;
  for (const word of s.split(" ")) {
    const ww = italW(word + " ", size);
    if (run > 0 && run + ww > w) (lines++, (run = ww));
    else run += ww;
  }
  return lines;
}

const outCubic = (x: number) => 1 - (1 - clamp01(x)) ** 3;
/** how open a reveal is at `t`: opens over `open` s from `at`, closes over `shut` s from `until` */
export const shown = (t: number, at: number, until?: number, open = 1.0, shut = 0.6) =>
  outCubic((t - at) / open) * (until === undefined ? 1 : 1 - smooth((t - until) / shut));

/** `text`'s letters arriving one by one from `at`, `step` s apart, each over `dur` s */
export function Letters({ text, t, at, step = 0.03, dur = 0.45 }: { text: string; t: number; at: number; step?: number; dur?: number }) {
  return (
    <>
      {[...text].map((c, i) => (
        <span key={i} style={{ opacity: smooth((t - at - i * step) / dur) }}>{c}</span>
      ))}
    </>
  );
}

/**
 * A block `w` x `h` at `left`/`top` (or `right`), open by `u`: two gold rods part from its middle and the words are
 * revealed between them, held where they are. `align` sets the words in the block. `pool` is how dark the shadow
 * behind it gets.
 */
export function Reveal({ u, w, h, left, right, top, align = "left", pool = 0.6, children }: {
  u: number; w: number; h: number; left?: number; right?: number; top: number; align?: "left" | "right" | "center"; pool?: number; children: ReactNode;
}) {
  if (u <= 0) return null;
  const open = Math.max(1, u * w), x0 = (w - open) / 2;
  // the rods are bright while they move and settle to a faint frame once open
  const rod = 1 - 0.55 * smooth((u - 0.85) / 0.15);
  const rodStyle = (x: number) => ({
    position: "absolute" as const, left: x, top: -16, width: 1.5, height: h + 32, opacity: rod * smooth(u / 0.08),
    background: `linear-gradient(180deg, rgba(214,185,126,0), ${gold} 25%, ${gold} 75%, rgba(214,185,126,0))`,
  });
  return (
    <div style={{ position: "absolute", ...(right !== undefined ? { right } : { left: left ?? 0 }), top, width: w, height: h }}>
      <div style={{ position: "absolute", left: -w * 0.35, top: -h * 0.9, width: w * 1.7, height: h * 2.8, opacity: smooth(u / 0.5), background: `radial-gradient(closest-side, rgba(0,0,0,${pool}), rgba(0,0,0,${pool * 0.45}) 55%, rgba(0,0,0,0))` }} />
      {/* the mask: as wide as the rods are apart, and tall enough never to cut an ascender, a descender or a shadow */}
      <div style={{ position: "absolute", left: x0, top: -h, width: open, height: 3 * h, overflow: "hidden" }}>
        <div style={{ position: "absolute", left: -x0, top: h, width: w, height: h, display: "flex", flexDirection: "column", justifyContent: "center", alignItems: align === "left" ? "flex-start" : align === "right" ? "flex-end" : "center" }}>
          {children}
        </div>
      </div>
      <div style={rodStyle(x0 - 14)} />
      <div style={rodStyle(x0 + open + 12)} />
    </div>
  );
}

/** tracked capitals; `track` in em, its trailing space taken back so the block's edge is the last letter's */
export function Caps({ children, size, track = 0.3, weight = 500, color = ivory }: { children: ReactNode; size: number; track?: number; weight?: number; color?: string }) {
  return <div style={{ fontFamily: serif, fontWeight: weight, fontSize: size, letterSpacing: `${track}em`, marginRight: `-${track}em`, lineHeight: 1, color, textShadow: shade }}>{children}</div>;
}
export function Italic({ children, size, weight = 400, color = ash, style }: { children: ReactNode; size: number; weight?: number; color?: string; style?: Record<string, unknown> }) {
  return <div style={{ fontFamily: serif, fontStyle: "italic", fontWeight: weight, fontSize: size, lineHeight: 1.1, color, textShadow: shade, ...style }}>{children}</div>;
}
/** a short gold rule that draws out from the block's edge as it opens */
export function Rule({ u, w = 44, align = "left" }: { u: number; w?: number; align?: "left" | "right" | "center" }) {
  return <div style={{ width: w * smooth((u - 0.3) / 0.6), height: 1, background: gold, margin: "14px 0 12px", alignSelf: align === "left" ? "flex-start" : align === "right" ? "flex-end" : "center" }} />;
}
