// A composition using a story explicitly (docs/story.feature ST5a): each beat's frames from the telling (a beat's
// `seconds`, else `each`), and <Beat>, a Sequence named by the beat's id so raea check's story check finds it.
import { Sequence } from "raea";
import type { ReactNode } from "react";
import { resolved } from "../load.ts";
import type { AnyStory } from "../model.ts";

export type BeatTime = { key: string; from: number; durationInFrames: number };

/** Every beat of a telling with its first frame and length, laid end to end. */
export function beatTimes(s: AnyStory, telling: string | undefined, fps: number, each = 3, seed = 0): BeatTime[] {
  const name = telling ?? Object.keys(s.tellings)[0];
  let from = 0;
  return (resolved(s, seed).tellings[name] ?? []).map((b, i) => {
    const durationInFrames = Math.round((b.seconds ?? each) * fps);
    const t = { key: b.id ?? `b${i}`, from, durationInFrames };
    from += durationInFrames;
    return t;
  });
}

/** One beat's frames, by its key. */
export function beatTime(times: BeatTime[], key: string): BeatTime {
  const t = times.find((x) => x.key === key);
  if (!t) throw new Error(`no beat ${key} in this telling (it has ${times.map((x) => x.key).join(", ")})`);
  return t;
}

/** A beat on screen: a Sequence named by its key, at its frames. */
export function Beat({ times, id, children }: { times: BeatTime[]; id: string; children?: ReactNode }) {
  const t = beatTime(times, id);
  return (
    <Sequence name={t.key} from={t.from} durationInFrames={t.durationInFrames}>
      {children}
    </Sequence>
  );
}
