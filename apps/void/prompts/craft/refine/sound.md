# Refine: sound

Half the piece is heard. Many viewers start muted; the ones who unmute decide in a second whether it sounds made or
assembled. The voice comes first, then everything else serves moments.

## Diagnose
Listen once with the picture off. Mark: where the voice is hard to follow, where the music fights it, where a
sound is there because a part entered rather than because something happened, where the piece goes silent by
accident, and where it should go quiet and doesn't.

## Fix
- **The voice.** Clear and even across cuts: a join between takes shouldn't jump in level or room tone. When two
  reads don't match, choose a different read before reaching for gain.
- **The bed, if there is one.** Its character comes from DIRECTION.md's SOUND (music from Tone.js instruments: TokenSynth or a sample library, `motion/music`). It sits well
  under the voice (somewhere around -21 dB under talk is a starting point, judged by ear) and ducks while words are
  spoken (a `volume` function over `useWords()`; `raea craft motion/sound`). It enters and leaves with intent: under
  the hook or after it, out before the last line or on it. No bed is a valid choice.
- **Foley as punctuation.** One family of sound (SAO) for the piece, used only on designed moments that carry
  meaning: the proof landing, the turn, the ask. A sound on every entrance stops meaning anything.
- **Silence.** A beat of quiet before the payoff, or a drop of the bed under the key line, lands harder than any
  effect.
- **Narration over the picture** (Fish) only when the piece needs a voice the footage doesn't have.
- **Master.** The floor's loudness (`<Mix>`), checked after encoding.

## Verify
Listen again picture-off: every word followed without effort, every sound placed for a reason you can name, the
master on target.

next: stages/polish
