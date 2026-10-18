// raea add statcard: a number and up to three lines beside it (*word* marks the accent), and up to two labelled bars
// that fill on the words that say them. Use it only when the footage says a real number; every claim comes from the
// brand's own material or the take. *_say props are phrases from the transcript, word for word.
import { Easing, Region, findPhrase, interpolate, useCurrentFrame, useWords } from "@raea/motion";

type Bar = { label: string; value: number; say?: string };
type Props = { big: string; bigSay?: string; l1?: string; l2?: string; l3?: string; bars?: Bar[]; region?: "auto" | "top" | "bottom" | "center" };

const accent = (s: string) => s.split(/(\*[^*]+\*)/).map((p, k) => (p.startsWith("*") ? <span key={k} className="text-brand-accent">{p.slice(1, -1)}</span> : <span key={k}>{p}</span>));

export default function StatCard({ big, bigSay, l1, l2, l3, bars = [], region = "top" }: Props) {
  const f = useCurrentFrame();
  const words = useWords();
  const when = (say: string | undefined, d: number) => {
    if (!say) return d;
    try { return findPhrase(words, say); } catch { return d; }
  };
  const ease = (from: number, len = 10) => interpolate(f - from, [0, len], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.bezier(0.05, 0.7, 0.1, 1) });
  const t = ease(when(bigSay, 0));
  return (
    <Region name={region}>
      <div style={{ opacity: t, transform: `translateY(${(1 - t) * 40}px)` }} className="mx-auto w-[86%] rounded-brand bg-brand-surface p-10 font-brand text-brand-on-surface shadow-xl">
        <div className="flex items-center gap-8">
          <div className="text-display font-black leading-none text-brand-accent">{big}</div>
          <div className="flex flex-col text-body font-bold leading-snug">
            {l1 ? <div>{accent(l1)}</div> : null}
            {l2 ? <div>{accent(l2)}</div> : null}
            {l3 ? <div>{accent(l3)}</div> : null}
          </div>
        </div>
        {bars.map((b, i) => {
          const fill = ease(when(b.say, 12 + i * 10), 18);
          return (
            <div key={i} className="mt-6">
              <div className="mb-2 flex justify-between text-label font-bold uppercase"><span>{b.label}</span><span>{Math.round(b.value * fill)}%</span></div>
              <div className="h-5 w-full rounded-full bg-brand-3"><div className="h-5 rounded-full bg-brand-accent" style={{ width: `${b.value * fill}%` }} /></div>
            </div>
          );
        })}
      </div>
    </Region>
  );
}
