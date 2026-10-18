# Animating

## What to do

- Drive every moving value from the frame with `interpolate(frame, input, output, options)`. Clamp both sides:
  `{extrapolateLeft: "clamp", extrapolateRight: "clamp"}`. With several keyframes, `easing` can be one curve for
  every segment or an array with one curve per segment.
- Settle into place with `Easing.out(Easing.cubic)`, the ease the approved pieces were measured to use. Use
  `Easing.inOut(Easing.cubic)` for a move between two resting places (a wipe, a pan) and `Easing.in(Easing.cubic)`
  for an exit that leaves. Keep `Easing.linear` for constant motion: push-ins, progress bars, tickers.
- Use `spring({frame, fps, config})` when a move should feel physical. The default (mass 1, damping 10,
  stiffness 100) overshoots noticeably. `damping: 20` with `stiffness: 100` settles with no overshoot (critical),
  and about 14 to 16 gives a slight one. `durationInFrames` stretches a spring to land on that frame, `delay` holds
  it at `from` first, and `reverse` plays it backwards. `measureSpring({fps, config})` says how many frames a
  spring takes to settle.
- Write a small key-row helper when a part has many keyed values. Every approved piece has one: `key(f, keys,
  easing)` sorts the keys of an object like `{0: 30, 6: 0}` and interpolates through them, clamped, so
  `key(local, {0: 30, 6: 0})` is a 30 px rise over 6 frames and `key(local, {0: 0.6, 4: 1.08, 8: 1})` is a pop. The
  example below has it in full.

- Start from timings the approved pieces use, then vary them for the piece, because one move everywhere is a lint:
  - rise in: y offset 30 to 0 over 6 frames, out-cubic;
  - pop: scale 0.6, then 1.08 at frame 4, then 1 at frame 8;
  - card arrival: 30 px slide over 7 frames;
  - bar fill: width from 0 over about 24 frames, out-cubic, starting as the number is said;
  - strike-through: width 0 to the text's width over 7 to 8 frames;
  - slam: scale 1.3, then 0.96 at frame 4, then 1 at frame 8, for a hit;
  - push-in on footage: 2 to 4% of scale across the clip, linear.
- Keep every animated style property present on every frame. Once a scale settles, write `scale(1)` rather than
  dropping the transform, and write `opacity: 1` rather than leaving opacity out. To remove something, unmount it
  (`f >= at ? <Part /> : null`).

## Why

`interpolate` extends the end segment's line by default, so an unclamped fade from 0 to 1 over frames 10 to 20 is
at 3 by frame 40 and at -1 at frame 0. Opacity above 1, negative widths and runaway
translations are almost always a missing clamp.

The plain curves in `Easing` ease in, which means slow first and fast at the end. That is backwards for an entrance.
`Easing.ease` is `bezier(0.42, 0, 1, 1)`, an ease-in as well, not the gentle in-and-out the name suggests.
`Easing.out(...)` turns any curve into its settling form.

The engine keeps a style property that React stops sending, so a box whose transform was dropped stays at the last
scale it was drawn with. The approved deck and game always write a transform and an opacity for that reason.

Bounce and elastic easing by reflex is on the floor's refuse list. Use them only when the idea is literally bouncy.

## Common mistakes

- No clamp: the value runs past its end.
- Input frames that don't strictly increase, such as two cues landing on the same frame (`[at, at]`).
  `interpolate` throws and names the pair. Guard with `Math.max(a + 1, b)`.
- An easing array whose length isn't `input.length - 1`: it throws.
- `Easing.cubic` used for an entrance (it starts slow and arrives fast). Use `Easing.out(Easing.cubic)`.
- `spring({frame})` without `fps`: it throws. Pass `fps` from `useVideoConfig()`.
- Damping 200 for "no bounce". That is heavily overdamped and creeps in slowly. Use about 20 at stiffness 100.
- `Math.random()` for jitter or particles. Every frame would draw different values. Make a seeded value from the
  index instead: `const x = Math.sin(i * 12.9898) * 43758.5453; const r = x - Math.floor(x);` gives the same `r` in
  [0, 1) for particle `i` on every frame.
- Animating with CSS transitions or keyframes. The page is rendered fresh each frame, so CSS animation does
  nothing. Compute the value for the frame.
- The same pop on every part. Vary the entrance by the part's role.

## Example: a stat that lands, counts up and fills its bar

```tsx
import { AbsoluteFill, Composition, Easing, interpolate, registerRoot, spring, useCurrentFrame, useVideoConfig } from "@raea/motion";
import { css } from "./styles.gen";

const OUT = Easing.out(Easing.cubic);
type Keys = Record<number, number>;

/** A key row: the value at frame f, eased per segment, held before the first key and after the last. */
function key(f: number, keys: Keys, easing = OUT): number {
  const xs = Object.keys(keys).map(Number).sort((a, b) => a - b);
  if (xs.length === 1) return keys[xs[0]];
  return interpolate(f, xs, xs.map((x) => keys[x]), { easing, extrapolateLeft: "clamp", extrapolateRight: "clamp" });
}

/** A card that rises in, a number that pops and counts, and a bar that fills after it. */
export default function Stat({ value = 73, label = "of tests ship" }: { value?: number; label?: string }) {
  const f = useCurrentFrame();
  const { fps } = useVideoConfig();
  const pop = spring({ frame: f - 4, fps, config: { damping: 15, stiffness: 120 } });
  const count = Math.round(key(f, { 4: 0, 28: value }));
  const fill = key(f, { 10: 0, 34: value / 100 });
  return (
    <AbsoluteFill style={{ backgroundColor: "var(--brand-background)" }}>
      <style>{css}</style>
      <div style={{
        position: "absolute", left: 90, top: 640, width: 900, height: 460, boxSizing: "border-box", padding: 56,
        borderRadius: 44, backgroundColor: "var(--brand-accent)",
        transform: `translateY(${key(f, { 0: 30, 7: 0 })}px)`, opacity: key(f, { 0: 0, 5: 1 }),
      }}>
        <div style={{ fontFamily: "var(--brand-font-heading)", fontWeight: 800, fontSize: 220, lineHeight: 1, color: "var(--brand-background)", transform: `scale(${0.6 + 0.4 * pop})`, transformOrigin: "0% 50%" }}>
          {count}%
        </div>
        <div style={{ marginTop: 16, fontFamily: "var(--brand-font)", fontWeight: 700, fontSize: 64, color: "var(--brand-background)" }}>{label}</div>
        <div style={{ position: "absolute", left: 56, right: 56, bottom: 56, height: 28, borderRadius: 14, backgroundColor: "rgba(0,0,0,0.18)" }}>
          <div style={{ height: 28, borderRadius: 14, width: `${fill * 100}%`, backgroundColor: "var(--brand-background)" }} />
        </div>
      </div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="stat" component={Stat} width={1080} height={1920} fps={30} durationInFrames={60} />);
```
