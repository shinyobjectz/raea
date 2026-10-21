// A narrator that talks while you play (@raea/motion/speech): lines asked for on a timer and said live by Pocket TTS,
// captions that follow the voice word by word (speechAt, folded into the state), and an interruption: at CUT_AT (or
// when Space is pressed) the narrator stops at the next word and a quick reaction follows straight after the cut.
// The game is a pure fold over its input; the speech read-backs are pure too, so a render of a logged play is the same
// frames and the same sound (sdk/react/bench/speech/check.sh).
import { AbsoluteFill, keyEdges, useCurrentFrame, useGameState, type FrameInput } from "@raea/motion";
import { Speech, prepareSpeech, speechAt, speechStart, type Line } from "@raea/motion/speech";

export const FPS = 60;

interface Said extends Line {
  id: string;
  /** the frame it was asked for, the frame it's heard from, the frame it was interrupted at */
  asked: number;
  start: number;
  cut: number | null;
  /** what the read-back said last frame: the word being spoken, whether it has ended, where it ends */
  word: number;
  ended: boolean;
  endFrame: number | null;
  words: string[];
}

export interface Game {
  said: Said[];
  /** the next scripted line */
  next: number;
  keys: string[];
  /** frames the narrator was interrupted at */
  cuts: number[];
}

const VOICE = "watts";
/** the lines, and the frame each is asked for (each waits for the one before it to end) */
const SCRIPT: { id: string; text: string; at: number }[] = [
  { id: "begin", text: "Every journey begins with the strange business of standing up. Nobody tells you how.", at: 30 },
  { id: "tilt", text: "Now the path tilts. The hero always thinks the mountain is in the way. The mountain thinks it is the way.", at: 0 },
  { id: "stones", text: "Stone, then nothing, then stone. Life is mostly the nothing in between, and that's perfectly all right.", at: 0 },
  { id: "home", text: "So here you are, at the end, which is exactly where you started.", at: 0 },
];
const QUIPS = ["Oh! Careful.", "Whoa, whoa.", "Ah. You fell. Good."];
/** the scripted interruption: the second line is cut here (a fall, in a real game) */
export const CUT_AT = 30 + 60 * 9;

export const INITIAL: Game = { said: [], next: 0, keys: [], cuts: [] };

function ask(s: Game, id: string, text: string, frame: number, start = speechStart(frame, FPS)): Said {
  const line: Said = { id, text, voice: VOICE, asked: frame, start, cut: null, word: -1, ended: false, endFrame: null, words: [] };
  prepareSpeech(line);
  void s;
  return line;
}

export function step(s: Game, input: FrameInput, frame: number): Game {
  let said = s.said;
  let next = s.next;
  const cuts = s.cuts.slice();
  // where each line is (pure: the same in play and render)
  said = said.map((l) => {
    if (l.ended) return l;
    const at = speechAt(l, frame, FPS);
    return at.word === l.word && at.ended === l.ended && at.endFrame === l.endFrame && at.words.length === l.words.length ? l : { ...l, word: at.word, ended: at.ended, endFrame: at.endFrame, words: at.words };
  });
  const speaking = said.find((l) => !l.ended);
  // an interruption: the line being said is cut at the next word, and a reaction follows straight after it
  const pressed = keyEdges(s.keys, input.keys).includes("keydown: ");
  if (speaking && speaking.cut === null && !speaking.id.startsWith("quip") && (frame === CUT_AT || pressed)) {
    const cut = { ...speaking, cut: frame };
    const at = speechAt(cut, frame, FPS);
    const quip = QUIPS[cuts.length % QUIPS.length];
    said = said.map((l) => (l === speaking ? { ...cut, endFrame: at.endFrame } : l));
    said = [...said, ask(s, `quip${cuts.length}`, quip, frame, Math.max(speechStart(frame, FPS), (at.endFrame ?? frame) + 2))];
    cuts.push(frame);
  } else if (!speaking && next < SCRIPT.length) {
    // the next line, once the last has ended (and a breath after it)
    const last = said[said.length - 1];
    const due = next === 0 ? SCRIPT[0].at : (last?.endFrame ?? frame) + 20;
    if (frame >= due) {
      said = [...said, ask(s, SCRIPT[next].id, SCRIPT[next].text, frame)];
      next++;
    }
  }
  // lines long finished are let go
  said = said.filter((l) => !l.ended || frame - (l.endFrame ?? frame) < 90);
  return { said, next, keys: input.keys ?? [], cuts };
}

/** the fold with the narrator silent */
const quietStep = (s: Game, input: FrameInput): Game => ({ ...s, keys: input.keys ?? [] });

export function Narrator({ quiet }: { quiet?: boolean }) {
  const frame = useCurrentFrame();
  const g = useGameState(INITIAL, quiet ? quietStep : step);
  const shown = g.said.filter((l) => frame >= l.start).slice(-2);
  return (
    <AbsoluteFill style={{ background: "#101418", fontFamily: "Inter", color: "#e9ecf2" }}>
      {g.said.filter((l) => !l.ended).map((l) => <Speech key={l.id} text={l.text} voice={l.voice} start={l.start} cut={l.cut} />)}
      <div style={{ position: "absolute", left: 40, right: 40, top: 60, fontSize: 26, lineHeight: 1.35 }}>
        {shown.map((l) => (
          <div key={l.id} style={{ marginBottom: 18, opacity: l.ended ? 0.35 : 1 }}>
            {l.text.split(/\s+/).map((w, i) => (
              <span key={i} style={{ opacity: i <= l.word ? 1 : 0.18, color: l.cut !== null && i > l.word ? "#ff8a7a" : undefined }}>{w + " "}</span>
            ))}
          </div>
        ))}
      </div>
      <div style={{ position: "absolute", left: 40, bottom: 30, fontSize: 16, opacity: 0.6 }}>
        frame {frame} · {g.said.filter((l) => !l.ended).map((l) => `${l.id} word ${l.word}${l.cut !== null ? ` cut@${l.cut}->${l.endFrame}` : ""}`).join(" · ") || "quiet"}
      </div>
    </AbsoluteFill>
  );
}
