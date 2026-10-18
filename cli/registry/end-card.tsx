// raea add end-card: the closing card: the brand's name or logo, one line, and the ask (follow, shop, try). Give it the
// last 2 to 3 seconds in a <Sequence>; it fades in over the footage or a fill of the brand's background.
import { AbsoluteFill, Img, interpolate, useCurrentFrame } from "@raea/motion";

type Props = {
  name: string;
  /** a logo file under media/ (optional) */
  logo?: string;
  line?: string;
  /** the ask: "Shop now", "Link in bio" */
  cta?: string;
  /** cover the footage with the brand's background (default) or sit on it */
  fill?: boolean;
};

export default function EndCard({ name, logo, line, cta, fill = true }: Props) {
  const f = useCurrentFrame();
  const t = interpolate(f, [0, 12], [0, 1], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ opacity: t }} className={`${fill ? "bg-brand-bg" : ""} flex flex-col items-center justify-center gap-8 px-16 text-center font-brand text-brand-ink`}>
      {logo ? <Img src={logo} className="h-40 object-contain" /> : <div className="text-display font-black">{name}</div>}
      {line ? <div className="text-title font-bold">{line}</div> : null}
      {cta ? <div className="rounded-full bg-brand-accent px-12 py-5 text-title font-extrabold text-brand-on-accent">{cta}</div> : null}
    </AbsoluteFill>
  );
}
