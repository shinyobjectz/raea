# Rendering and checking: raea render, raea check, stills

## What to do

- **`raea check` after every real change.** It renders edit.tsx (with words.json and perception.json beside it),
  then measures the render: cuts inside words, endings, dead air, frozen or black frames, text cut off by the frame,
  parts over the face, a flat picture, and the brief's checks. Each fault comes with its fix. An exception in the
  composition (an unclamped value going NaN, keyframes out of order, a phrase `word()` can't find, a missing prop)
  fails the render and names the line.
- **`raea check --full` once the piece is close.** It adds the model checks, which watch the render the way a
  viewer would: a frame cropped too tight, speech without captions, text that's hard to read, an ending that stops
  mid-thought, whether the message lands. They take longer, so run them before you answer, not after every change
  (the gate at the end of a turn runs them for you).
- **Stills to look at.** `raea still 0.5 2.1 4.8 9.0` writes frames of the render to `.stills/`; `raea sheet` makes a
  contact sheet of the whole. Pick the moments that matter (each cut, each part's entrance and settled state, the
  hook at 0.5 s, the end card) and `look` at them in one batch. Fix everything they show, then look again. Don't take
  one still per tweak.
- **`raea render`** writes final.mp4 alone, without the measurements. `raea render NAME` renders another
  deliverable, NAME.tsx, to NAME.mp4 (and `raea check NAME` checks it).
- **Loudness** comes out at -14 LUFS integrated (±1) with true peak at or below -1 dBTP when `<Mix>` is in the tree.

## Why

The engine renders the same tree the person will get, so a still from it is the truth about layout: fonts, footage,
3D and text overflow only show in pixels. `raea check` measures what a still can't (a clipped word, a frame of
black, dead air), so the two together cover the floor.

Looking in batches is the craft rule. One round of looking at the frames that matter, fixing everything, then
looking again, finds more than a still per change and costs less.

## Common mistakes

- Judging layout from the TSX alone. Text overflow and contrast against footage only show in pixels.
- Checking frame 0 only. Most bugs are at cues and joins.
- Calling it done without listening. The voice must stay intelligible over the bed, and every sound should land on
  its beat.
- Answering while `raea check` still names a fault.
