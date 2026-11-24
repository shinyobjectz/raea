// Three names a project moving to raea from another React video tool uses (#176, docs/move-in.feature MV1-MV3): a
// seeded `random`, `interpolateColors`, and `<OffthreadVideo>` for `<Video>`.
import { interpolate, type InterpolateOptions } from "@raea/math/interpolate";
import { Video, type VideoProps } from "@raea/video/media";
import { parseColor } from "@raea/ui/canvas";

/**
 * A number in [0, 1) from a seed, a string or a number: the same on every frame, every run and every machine, so a
 * scatter seeded `random("star" + i)` holds still while the frames change. A number seeds as its text (1 and "1" give
 * the same). `null` is `Math.random()`, which raea seeds per frame. `hash01` is the same for integers only.
 */
export function random(seed: string | number | null): number {
  if (seed === null) return Math.random();
  if (typeof seed !== "string" && typeof seed !== "number") throw new TypeError(`random: the seed is a string or a number, not ${typeof seed}`);
  const text = String(seed);
  // FNV-1a over the UTF-16 units, then murmur3's finaliser so seeds a letter apart land far apart
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  h ^= h >>> 16;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return (h >>> 0) / 4294967296;
}

/**
 * `interpolate` for CSS colours: `input` mapped through `inputRange` to a colour between the two of `outputRange` it
 * falls between, mixed in sRGB with alpha, as `rgba(r, g, b, a)` (channels rounded to whole numbers). Any CSS colour
 * the canvas parses: a name, #rgb, #rrggbb(aa), rgb(a)() or hsl(a)(). Held at the end colours outside the range;
 * `options` can change that and add an easing, as `interpolate`'s.
 */
export function interpolateColors(input: number, inputRange: readonly number[], outputRange: readonly string[], options?: InterpolateOptions): string {
  if (!Array.isArray(outputRange)) throw new TypeError("interpolateColors: the output keyframes must be an array of colours");
  const colours = outputRange.map((c) => {
    const rgba = parseColor(String(c));
    if (!rgba) throw new TypeError(`interpolateColors: ${JSON.stringify(c)} isn't a CSS colour (a name, #rrggbb, rgb() or hsl())`);
    return rgba;
  });
  const opts: InterpolateOptions = { extrapolateLeft: "clamp", extrapolateRight: "clamp", ...options };
  const [r, g, b, a] = [0, 1, 2, 3].map((k) => interpolate(input, inputRange, colours.map((c) => c[k]), opts));
  const byte = (v: number) => Math.round(Math.min(255, Math.max(0, v)));
  return `rgba(${byte(r)}, ${byte(g)}, ${byte(b)}, ${Number(Math.min(1, Math.max(0, a)).toFixed(4))})`;
}

/** `<Video>` under the name a project moving from another React video tool may use for it, with its props. */
export const OffthreadVideo = Video;
/** `<OffthreadVideo>`'s props: `<Video>`'s. */
export type OffthreadVideoProps = VideoProps;
