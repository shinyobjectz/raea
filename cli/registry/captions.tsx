// raea add captions: the spoken words, a few at a time, the word being said lit. Reads the edit's words itself
// (useWords(): `raea cut` writes them beside cuts.ts). Sits in the bottom band (parts take the top by default, so they
// never share one), on the brand's surface with its own text colour, a pair the kit keeps readable on any footage.
// Styled only from the brand's tokens (brand/brand.css); change anything here, it's yours.
import { Region, useCurrentFrame, useWords } from "@raea/motion";

type Props = {
  /** words on screen at once */
  per?: number;
  /** where: "bottom" by default (parts default to "top"); "auto" keeps clear of the face */
  region?: "auto" | "top" | "bottom" | "center";
};

export default function Captions({ per = 3, region = "bottom" }: Props) {
  const f = useCurrentFrame();
  const words = useWords();
  const i = words.findIndex((w) => f >= w.from && f < w.to);
  // between words, the last group stays up until the next word starts
  const at = i >= 0 ? i : words.findLastIndex((w) => w.to <= f);
  if (at < 0) return null;
  const start = at - (at % per);
  const group = words.slice(start, start + per);
  if (group.length === 0 || f >= group[group.length - 1].to + 6) return null;
  return (
    <Region name={region}>
      <div className="flex justify-center">
        <div className="flex max-w-[90%] flex-wrap justify-center gap-x-[0.28em] rounded-brand bg-brand-surface px-8 py-4 text-center font-brand text-caption font-extrabold leading-tight text-brand-on-surface shadow-xl">
          {group.map((w, k) => (
            <span key={k} className={start + k === i ? "text-brand-accent" : ""}>{w.text}</span>
          ))}
        </div>
      </div>
    </Region>
  );
}
