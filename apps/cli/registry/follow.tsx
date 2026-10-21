// raea add follow: the ask as a pill that hugs its text ("Follow for part 2", "Shop the stack"). Give it the moment the
// speaker asks, or the last seconds.
import { Region, interpolate, useCurrentFrame } from "@raea/motion";

type Props = { text: string; region?: "auto" | "top" | "bottom" | "center" };

export default function Follow({ text, region = "top" }: Props) {
  const t = interpolate(useCurrentFrame(), [0, 10], [0, 1], { extrapolateRight: "clamp" });
  return (
    <Region name={region}>
      <div className="flex justify-center">
        <div style={{ opacity: t, transform: `scale(${0.85 + 0.15 * t})` }} className="rounded-full bg-brand-accent px-12 py-5 font-brand text-title font-extrabold text-brand-on-accent shadow-xl">{text}</div>
      </div>
    </Region>
  );
}
