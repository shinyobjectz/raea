// word() and useWords(): time anchored to what's said (spec: "Time anchored to words").
import { useClockState, useLocalTime, type TimelineWord } from "./clock";

export interface WordOptions {
  /** which time the phrase is said, from 1 */
  occurrence?: number;
  /** "start": the phrase's first frame; "end": the frame after its last word ends */
  edge?: "start" | "end";
}

/** Matching ignores case and punctuation: only letters and digits count. */
function normalise(text: string): string {
  return text.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");
}

function tokens(phrase: string): string[] {
  return phrase.split(/\s+/).map(normalise).filter((t) => t !== "");
}

/** Finds the composition frame where `phrase` is said in a transcript. */
export function findPhrase(words: readonly TimelineWord[], phrase: string, options: WordOptions = {}): number {
  const { occurrence = 1, edge = "start" } = options;
  if (!Number.isInteger(occurrence) || occurrence < 1) {
    throw new RangeError(`word("${phrase}"): occurrence must be a whole number from 1, got ${occurrence}`);
  }
  if (edge !== "start" && edge !== "end") throw new RangeError(`word("${phrase}"): edge must be "start" or "end"`);
  const wanted = tokens(phrase);
  if (wanted.length === 0) throw new Error(`word("${phrase}"): the phrase has no words to match`);

  // the transcript as matchable tokens, skipping entries that are only punctuation
  const heard = words.map((w) => ({ key: normalise(w.text), word: w })).filter((h) => h.key !== "");
  let found = 0;
  for (let i = 0; i + wanted.length <= heard.length; i++) {
    let ok = true;
    for (let j = 0; j < wanted.length; j++) {
      if (heard[i + j].key !== wanted[j]) {
        ok = false;
        break;
      }
    }
    if (ok && ++found === occurrence) {
      return edge === "start" ? heard[i].word.from : heard[i + wanted.length - 1].word.to;
    }
  }
  const which = found === 0 ? "is never said" : `is said only ${found} time${found === 1 ? "" : "s"}, not ${occurrence}`;
  throw new Error(`word("${phrase}"): the phrase ${which} on the timeline`);
}

/** The composition frame where `phrase` is said. Call it while rendering (it reads the Clock). */
export function word(phrase: string, options?: WordOptions): number {
  return findPhrase(useClockState("word").words, phrase, options);
}

/** Every word heard on the timeline, in the frames of the nearest enclosing Sequence. */
export function useWords(): TimelineWord[] {
  const clock = useClockState("useWords");
  const { origin } = useLocalTime("useWords");
  return clock.words.map((w) => ({ text: w.text, from: w.from - origin, to: w.to - origin }));
}
