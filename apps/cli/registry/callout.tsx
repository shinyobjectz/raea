// raea add callout: a short labelled card that slides in with the line it supports (a step, a claim, a stat), in the
// band away from the face. Put it in a <Sequence from durationInFrames> timed to the words (word("phrase") finds them).
import { Easing, Region, interpolate, useCurrentFrame } from "@raea/motion";

type Props = {
  /** the small line above: "Step 1", "Fact", "New" */
  label?: string;
  title: string;
  detail?: string;
  region?: "auto" | "top" | "bottom" | "center";
};

export default function Callout({ label, title, detail, region = "top" }: Props) {
  const f = useCurrentFrame();
  const t = interpolate(f, [0, 10], [0, 1], { extrapolateRight: "clamp", easing: Easing.bezier(0.05, 0.7, 0.1, 1) });
  return (
    <Region name={region}>
      <div style={{ opacity: t, transform: `translateY(${(1 - t) * 40}px)` }} className="mx-auto max-w-[86%] rounded-brand bg-brand-surface px-10 py-8 font-brand text-brand-on-surface shadow-xl">
        {label ? <div className="mb-2 text-label font-bold uppercase tracking-widest text-brand-accent">{label}</div> : null}
        <div className="text-title font-extrabold leading-tight">{title}</div>
        {detail ? <div className="mt-2 text-body opacity-80">{detail}</div> : null}
      </div>
    </Region>
  );
}
