# Live speech (@raea/motion/speech)

A game or a composition can say lines that are made while it plays: Pocket TTS (Kyutai's, on the engine's ONNX
Runtime through `raea-ml`) makes the audio on a worker thread, the frames that play a line stream its samples onto the
Tone graph's voice bus (so music ducks under it), and a word clock tells the game which word is being said. Nothing is
recorded ahead, so the text can be made at run time.

## Using it

```tsx
import { Speech, prepareSpeech, speechAt, speechStart } from "@raea/motion/speech";

// in the step, when the narrator should speak: ask now, be heard 0.15 s later
const line = { text: "Ah. You fell. Good.", voice: "watts" };
prepareSpeech(line);
s = { ...s, line: { ...line, start: speechStart(frame, FPS), cut: null } };

// each frame, in the step: where the line is (pure, so it may go into the state)
const at = speechAt(s.line, frame, FPS); // { started, word, ended, endFrame, words }

// interrupt it: it stops at the next word's start (at most 0.8 s on), and endFrame says where
s = { ...s, line: { ...s.line, cut: frame } };

// in the view: keep it on the page until it has ended
{!at.ended && <Speech {...s.line} />}
```

- **`Line`**: `text`, `voice` (a Pocket voice such as `"alba"` or `"marius"`, a clip in `~/.raea-void/voices` by name
  such as `"watts"`, or a WAV: `staticFile("narrator.wav")`; the default is `watts` when that clip exists, else
  `alba`), `seed` (another take; 0), `temperature` (0.7; lower is calmer).
- **`<Speech {...line} start cut volume />`**: the line heard from frame `start` (with `start` null it's only made
  ahead), stopped at the next word after frame `cut`. Taken off the page early, it stops at once with a 10 ms fade.
- **`speechAt(line, frame, fps)`**: `word` (an index into `words`, -1 before the first), `ended`, `endFrame` (known for
  a cut line, and for any line once it's all made) and `words`.
- **`speechStart(frame, fps)`**: `frame` + 0.15 s (`SPEECH_LEAD`). Start every line at least this far after it's
  asked for, including a reaction after a cut: `Math.max(speechStart(frame, FPS), endFrame + 2)`.
- **`prepareSpeech(line)`**: start making it now (call it as soon as the text is known).
- **`speechDuration(line)`**: its length in seconds (waits for all of it; pure). **`speechMade(line)`**: how much is
  made so far (display-only).

The worked example is the narrator test game, `sdk/react/bench/speech/game/`: four lines on a timer with word-by-word
captions, the second cut at frame 570 with a quick reaction after it, and Space to interrupt (`quiet.tsx` is the same
game with the narrator silent, for frame times).

## Why it plays as it renders

A line is named by its text, voice, seed and temperature. Pocket with a fixed seed says the same text the same way, so
its samples, the moment each word starts and where a cut lands depend only on the line, not on when the audio was
made. The word clock decides each moment from at most 180 ms of audio past it, and a cut waits until the loudness
around the next word's start is known. So `speechAt` gives the same answer in live play, in a render of a logged play
and in a fresh render, and a game can fold it into its state.

The one difference in live play is waiting. A render waits for every sample. Live play starts making a line the
moment it's asked for and holds a frame only if the worker hasn't got that far. Pocket makes its first audio in 25-70
ms and runs 5-8 times faster than real time on the M5 Pro, so a line started 0.15 s after it's asked for doesn't wait.
**A hold costs more than a late frame**: the mixer schedules each frame's audio only a little ahead of its sample
(two audio blocks, 5 ms and half a frame), so a frame held longer than that has its sound scheduled after its sample,
and that stretch of the play no longer equals the render. A line started two frames after it was asked for held its
frame 24-41 ms and broke the equality (found by `check.sh`); the cure is the lead, not a longer mixer head. The play report counts holds under `speech.waits`.

## Measured (M5 Pro, October 2026, `raea-motion play --offscreen`, speech cache off)

- First audio 25-71 ms a line; real-time factor 0.11-0.19.
- Frames held for speech: 0 in every session with the lead (three 40 s sessions, and six 20 s sessions with Space
  pressed at random).
- Frame work p99 with the narrator talking: 1.1 / 3.1 / 4.7 ms over three 40 s sessions; the silent game measured
  1.2 / 7.1 / 1.3 ms. The machine's load (7-14) moves these more than speech does. 60 fps held.
- The play's tap against a fresh render's bounce: max difference 0 over 40 s, cut and reaction included; the same for
  a randomly pressed play against the render of its logged input.
- Word timing is approximate. Against Parakeet on six lines the median error is 0.10 s, with 63% of words within
  150 ms: good enough for captions, not a forced aligner.

## Checking it

`sdk/react/bench/speech/check.sh` (build `raea-motion` first; `RAEA_MOTION` names another binary):

- **start**: the first line, asked for at frame 30, is silent before frame 39 and heard within 100 ms of it.
- **cut**: Parakeet hears "tilts" and "careful" around the cut and not the rest of the line ("mountain"). Skipped
  without `parakeet-mlx`.
- **live**: the play's tap (live inference) against a fresh render's bounce, at most 1e-6 apart.
- **logged**: a play with Space pressed at random, rendered from its logged input, at most 1e-6 apart.

It prints each play's speech report too, and its outputs (a few WAVs, two small MP4s) are deleted when it ends unless
`RAEA_SPEECH_KEEP=1`.

## The cache

Finished lines are kept in `~/.cache/raea-speech` (`RAEA_SPEECH_CACHE` moves it; `off` turns it off), so a line said
once is ready at once the next time. Its name covers the model bundle and the way it's said (`VERSION` in
`ai/ml/src/speech.rs`; bump it when either changes).

## Under the hood

- `ai/ml/src/pocket.rs`: Pocket TTS, resumable (`begin` / `step`) and seeded, so lines can be interleaved and cut.
- `ai/ml/src/speech.rs`: the worker (one piece of 160 ms or a sentence's start at a time, always for the line with the
  least audio made beyond where it's needed), the disk cache, `word_at` and `cut_at`.
- `ai/ml/src/words.rs`: the word clock (syllable peaks on a 10 ms loudness envelope, words placed by syllable counts).
- `sdk/react/src/ml.rs`: the `speech.*` native calls and `frame_speech`, each frame's speech for the mixer;
  `sdk/react/src/audio.rs` puts it in `ToneCue.speech`, and the mixer plays it on the voice bus.
- `sdk/motion/src/speech.tsx`: the SDK.

## Known gaps

- A cut line keeps being made to its end (the worker's time, not the sound).
- Word times are estimates, as above.
