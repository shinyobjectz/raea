<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: the Experience mode, rewritten for games. See NOTICE. -->
# Mode: game

An edit whose state moves frame by frame under input: a playable toy, a scripted play-through, a game shown as a
video.

## The viewer succeeds when
They understand the goal without being told, feel their input matter, and want another go. For a scripted
play-through shown as video, the watcher understands the rule in the first seconds and roots for the outcome.

## What ambition looks like
- **The world leads; the interface recedes.** The play space is the picture. Score, lives and prompts are small,
  consistent and out of the way of the action, set in the world's type.
- **Feedback within a few frames.** Every input gets an answer the player can see and hear within about 150 ms
  (four or five frames at 30 fps): a hit flashes, a jump sounds, a miss reads as a miss.
- **One rule, legible.** The core mechanic is readable from the first moments of play; complexity arrives one
  element at a time.
- **A world of its own.** Palette, shapes and sound from one direction, specific enough to recognise in a
  screenshot with the score removed. Generic primitives on a black ground are the rut.

## The rut to name
Default shapes and colours from the example it started from, a score in the corner in the default face, no sound.

## Watch for
- Determinism: the same input script plays the same way every time (randomness seeded from the frame or an index,
  never `Math.random()` or a clock; `raea craft motion/games`).
- Difficulty: the first seconds must be winnable; a game that ends before the rule is clear teaches nothing.
- Render it with a script of input and look at stills where things happen, not only the start.

Rubric weight: 3 (proof) and 4 (the ask) are n/a or become "the outcome lands"; 1, 5, 8 and 9 decide it. Add the
player as a fifth viewer: where do they get confused or bored?
