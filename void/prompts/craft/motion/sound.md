# Sound: Audio, lyria and sao, gains, fades, Mix

## What to do

- Put each sound in its own `<Sequence from={frame} layout="none" name>` with an `<Audio>` inside. The Sequence's
  `from` is when the sound starts, and its `durationInFrames` (optional) cuts it off. `layout="none"` because a sound
  draws nothing.
- Set levels in decibels with `gainDb`. `volume` is the same thing as a linear gain (1 is unity, 0.5 is about -6 dB,
  values above 1 boost). Either one can be a function of the sound's own frame (0 at its first frame) for a duck or
  a swell. `gainDb` wins when both are given.
- Fade with `fadeIn` and `fadeOut`, in seconds. They ramp from and to silence at the start and end of where the sound
  plays, at the audio's own rate, and multiply with the gain. Use about 1 to 1.4 s at the end of a bed, and
  0.012 s at a cut.
- Make music with `lyria(prompt, {seconds})` (30 s by default) and foley with `sao(prompt, {seconds})` (2 s by
  default), and pass the result to `<Audio src>`. Write prompts the way the approved pieces do: the sound, its
  character and what to leave out ("deep punchy cinematic impact hit, tight, no reverb tail"; "minimal confident
  modern tech beat for a sharp founder explainer"). Ask for the length you need.
- Master the sum with one `<Mix lufs={-14} ceiling={-1.5} />` per composition. It brings the whole mix to -14 LUFS
  integrated and limits true peaks under the ceiling, which is what the floor's loudness defect asks for.
- Put the bed first in the tree, then the other sounds in time order. The engine keys a sound's stretch by its place
  in the tree, so adding a sound in the middle of the list can split the ones after it.
- Balance relative to the voice. The approved pieces, mastered by `<Mix>` to -14 LUFS, sit like this:

  | sound | with a voice | without one (promo, deck, game) |
  |---|---|---|
  | the voice clips | about 0 dB | none |
  | music bed | -21 dB, rising to about -12 dB once the voice stops (the end card) | -12 to -15 dB |
  | foley hits, whooshes, pops | -16 to -22 dB | -1 to -6 dB |

  These are starting points; listen and decide.

## Why

A sound placed without a Sequence starts at frame 0 with everything else. A Sequence with the default layout adds a
full-frame box that paints over the parts beneath it.

`lyria()` and `sao()` return an asset `{kind, prompt, seconds, id, src}` whose id is a stable hash of the kind, the
prompt and the length. The renderer generates each asset once, under the run's paid-generation cap, and caches it by
id. The same call always reuses the same sound, and changing one word of the prompt pays for a new one. So settle
the prompt before you iterate on gains.

Policy: Lyria makes music, SAO makes foley, and Fish makes narration over the picture (it reaches the composition as
a file, `motion/assets`). No other generation.

Sound sells the cut, but one move everywhere is a lint: a whoosh on every entrance flattens all of them. Give
foley to the moments that are physical (a slam, a strike-through, a card landing on the beat) and let others be
silent.

## Common mistakes

- An `<Audio>` without a Sequence, or in a Sequence without `layout="none"`.
- `volume={-6}` meaning decibels. That throws, because volume is a non-negative linear gain. Write `gainDb={-6}`.
- A bed at -6 dB under the voice. The voice must stay intelligible over everything (floor).
- Two `<Mix>` in one composition: it throws.
- No `<Mix>`: the sum plays as it is, at whatever loudness it adds up to.
- Rewording a `lyria()` prompt on every pass. Each new prompt is a new paid generation.
- `style` on an `<Audio>`: Audio has no picture and takes no style.
- Hand-placing dozens of game sounds from a log. Cue them from `useGameEvents` (`motion/games`).

## Example: a bed that swells for the end card, a voice, foley on two beats

```tsx
import { AbsoluteFill, Audio, Composition, Easing, Mix, Sequence, interpolate, lyria, registerRoot, sao, staticFile } from "@raea/motion";
import { css } from "./styles.gen";

const OUT = Easing.out(Easing.cubic);
const BED = lyria("warm minimal electronic pulse for a founder explainer, soft kick, no vocals", { seconds: 12 });
const HIT = sao("deep punchy impact hit, tight, no reverb tail", { seconds: 1.2 });
const POP = sao("bubbly UI pop click, short, dry", { seconds: 0.6 });
const VOICE = staticFile("voice/vo01.wav"); // narration made earlier, as a file

const FRAMES = 300;
const END_CARD = 252; // the voice stops here

/** Every sound after the bed, in time order. */
const SOUNDS = [
  { id: "hit", from: 0, gainDb: -16, src: HIT, fade: 0 },
  { id: "voice", from: 15, to: END_CARD, gainDb: 0, src: VOICE, fade: 0.012 },
  { id: "pop", from: END_CARD + 2, gainDb: -17, src: POP, fade: 0 },
];

/** The bed in dB at its own frame m: under the voice, then up for the end card. */
const bedDb = (m: number) =>
  interpolate(m, [END_CARD - 10, END_CARD], [-21, -12], { easing: OUT, extrapolateLeft: "clamp", extrapolateRight: "clamp" });

export default function SoundBed() {
  return (
    <AbsoluteFill style={{ backgroundColor: "var(--brand-background)" }}>
      <style>{css}</style>
      <Sequence from={0} durationInFrames={FRAMES} name="bed" layout="none">
        <Audio name="bed" src={BED} gainDb={bedDb} fadeOut={1.2} />
      </Sequence>
      {SOUNDS.map((s) => (
        <Sequence key={s.id} from={s.from} durationInFrames={s.to ? s.to - s.from : undefined} name={s.id} layout="none">
          <Audio name={s.id} src={s.src} gainDb={s.gainDb} fadeIn={s.fade} fadeOut={s.fade} />
        </Sequence>
      ))}
      <Mix lufs={-14} ceiling={-1.5} />
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="sound" component={SoundBed} width={1080} height={1920} fps={30} durationInFrames={FRAMES} />);
```
