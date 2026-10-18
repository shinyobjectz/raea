// raea add lockup: a few lines stacked big, each landing as it's said. " / " splits the lines; *word* marks the accent.
// Put it in a <Sequence> over the words it echoes; each line appears on its first word (or one by one, 8 frames apart,
// when the line isn't said). Styled only from the brand's tokens.
import { Easing, Region, findPhrase, interpolate, useCurrentFrame, useWords } from "@raea/motion";

type Props = { text: string; region?: "auto" | "top" | "bottom" | "center"; small?: boolean };

export default function Lockup({ text, region = "top", small = false }: Props) {
  const f = useCurrentFrame();
  const words = useWords();
  const lines = text.split(" / ");
  const at = (line: string, i: number) => {
    try {
      return findPhrase(words, line.replace(/\*/g, "").split(/\s+/).slice(0, 2).join(" "));
    } catch {
      return i * 8;
    }
  };
  return (
    <Region name={region}>
      <div className={`flex flex-col items-center gap-2 text-center font-brand font-black uppercase leading-none text-brand-ink [text-shadow:0_4px_24px_rgba(0,0,0,0.5)] ${small ? "text-title" : "text-display"}`}>
        {lines.map((line, i) => {
          const t = interpolate(f - at(line, i), [0, 8], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: Easing.bezier(0.05, 0.7, 0.1, 1) });
          return (
            <div key={i} style={{ opacity: t, transform: `translateY(${(1 - t) * 30}px) scale(${0.92 + 0.08 * t})` }}>
              {line.split(/(\*[^*]+\*)/).map((part, k) => (part.startsWith("*") ? <span key={k} className="text-brand-accent">{part.slice(1, -1)}</span> : <span key={k}>{part}</span>))}
            </div>
          );
        })}
      </div>
    </Region>
  );
}
