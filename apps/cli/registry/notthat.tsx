// raea add notthat: a lead statement, then the idea it rejects on a card, struck through when the speaker says
// strikeSay ("not X, but Y"). strikeSay is a phrase from the transcript, word for word.
import { Easing, Region, findPhrase, interpolate, useCurrentFrame, useWords } from "@raea/motion";

type Props = { lead: string; not: string; strikeSay?: string; region?: "auto" | "top" | "bottom" | "center" };

// " / " splits lines and *word* marks the accent, as in a lockup
const lines = (t: string) =>
  t.split(" / ").map((line, i) => (
    <div key={i}>{line.split(/(\*[^*]+\*)/).map((p, k) => (p.startsWith("*") ? <span key={k} className="text-brand-accent">{p.slice(1, -1)}</span> : <span key={k}>{p}</span>))}</div>
  ));

export default function NotThat({ lead, not, strikeSay, region = "top" }: Props) {
  const f = useCurrentFrame();
  const words = useWords();
  let at = 24;
  if (strikeSay) {
    try { at = findPhrase(words, strikeSay); } catch { at = 24; }
  }
  const ease = (from: number, len = 10) => interpolate(f - from, [0, len], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.bezier(0.05, 0.7, 0.1, 1) });
  const card = ease(6);
  const strike = ease(at, 8);
  return (
    <Region name={region}>
      <div className="flex flex-col items-center gap-6 text-center font-brand">
        <div style={{ opacity: ease(0) }} className="rounded-brand bg-brand-surface px-8 py-4 text-title font-extrabold text-brand-on-surface">{lines(lead)}</div>
        <div style={{ opacity: card, transform: `scale(${0.9 + 0.1 * card})` }} className="relative rounded-brand bg-brand-surface px-10 py-6 text-title font-extrabold text-brand-on-surface shadow-xl">
          <span style={{ opacity: 1 - 0.45 * strike }}>{lines(not)}</span>
          <div className="absolute left-6 right-6 top-1/2 h-2 rounded-full bg-brand-accent" style={{ transform: `scaleX(${strike})`, transformOrigin: "left" }} />
        </div>
      </div>
    </Region>
  );
}
