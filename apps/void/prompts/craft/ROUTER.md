<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0; rewritten for timed video. See NOTICE. -->
# Craft: how to design a piece

What you build, you also design: short videos, motion pieces, decks and games, from real footage and real facts.
Bring a creative director's point of view to every piece, chosen for this brand, this idea and this viewer, and
finish the craft. Nothing in these files is a look to copy. They are ways to decide. The decisions are yours, made per
piece, and you write them down.

Two rules sit above everything else:
- **The person and the brief win.** A colour, face, structure or line the person asked for, or one BRAND.md commits
  to, beats your taste and beats every lint. Steering a clear brief toward your own taste is a failure.
- **Refining keeps; redesigning replaces.** A change request ("make the hook punchier", "quieter music") keeps the
  piece's world and everything outside the ask. A redesign keeps the facts and the footage, and treats the old look
  as evidence, not authority. Never polish a look you are about to throw away.

## What lives in the object
- `BRAND.md`: the brand's truth: offer, audience, allowed claims with their evidence, voice, palette roles, faces and
  logos. `raea brand` writes it. Read it; never restate it. If it's missing, see `stages/brand`.
- the brief: the piece's strategy and words: mode, angle, hook, the beats, and the lines quoted from takes
  (`stages/strategy`). `raea script` compiles it.
- `DIRECTION.md`: this piece's visual and sonic direction: the thesis, its world, the first 1.5 s and the sound. It is
  never rendered into the piece.
- `critique/<n>.md`: your own critiques, so the next pass sees the trend.

## Pick the mode
The mode names what success looks like for the viewer. Pick it from what is being made and where it runs, not from
the product: a tool's ad is direct-response, while the same tool's walkthrough is app-demo.

| Mode | The viewer succeeds when they... | Signals |
|---|---|---|
| direct-response | stop, believe one claim, act | an ad, an offer, a CTA, paid placement |
| organic | watch to the end and feel they know the person | a creator post, a story, no hard ask |
| explainer | understand one mechanism | "how", "why", a process, numbers to make clear |
| deck | remember three points | slides, a pitch, pages to export |
| game | want another go | input, rules, a score |
| app-demo | see the job done in the real product | screen recordings, a feature, a walkthrough |

Write the mode in the brief. A piece has one mode. A blend means you haven't decided whose
success it serves.

Games, decks and motion pieces with no takes are written as TSX by hand: `raea script` compiles only briefs said in
footage. For those, read the mode and its motion craft (`motion/games` for a game); don't study takes the piece doesn't
use.

## First: the brand
Before any strategy or design, run `raea brand` with the brand's domain or name. Do this when the request names a brand
or product, or its logo or name shows in the footage, and the object has no BRAND.md. Write a provisional BRAND.md
yourself (`stages/brand`) only when no brand is named or the command finds none. A piece for no brand at all (a game
for fun, a personal deck) needs no BRAND.md.

## Pick the stages
- **A new piece:** the brand (above), then `stages/strategy`, `stages/direct`, `stages/build`, `stages/critique` and
  `stages/polish`. Each stage file ends by naming the next one. Read a stage when you start it and do its work
  before you read the next; the build stage is where the TSX gets written, and a turn that only reads has built
  nothing.
- **A change to a piece that works:** the refine playbook that matches the ask, then `stages/polish`. Use
  `refine/pace` for pacing, dead air or length; `refine/sound` for music, foley, loudness or voice (and `motion/music` to write music or a game's sounds); `refine/punch` for
  "bolder", "more punch" or a flat beat; `refine/quieter` for "too much", busy or loud; `refine/distill` for too long
  or too many ideas; `refine/typeset` for type, captions or hierarchy; `refine/colorize` for colour, contrast or
  grade; `refine/animate` for motion, entrances or the timing of moves; `refine/layout` for placement, bands or other
  shapes.
- **A quick edit** (one value, one word, one cut): just make it. The floor still applies.
- **"This isn't working"** or a different idea entirely: that's a redesign. Go back to `stages/strategy`.

Read a file with `raea craft NAME` (`raea craft modes/organic`, `raea craft stages/direct`). `raea craft` with no
name lists them all, including `motion` and its rules (`motion/footage`, `motion/words` and the rest), which cover
writing the TSX itself, `motion/music` for music, sound effects, buses and ducking (Tone.js on raea), and `bevy` and its topics (`bevy/scene`, `bevy/lights`, `bevy/materials`, `bevy/games` and
the rest) for anything 3D: scenes, product shots, models, 3D games. Read the mode file once, when the mode is chosen, then one stage or playbook at a time. The
floor and this router are always in front of you.

## How judgement works here
- The floor's lints inform. A lint names a risk and how it's measured; you decide. When you keep something a lint
  flags, the idea must earn it, and DIRECTION.md says why in one line. Defects (a clipped word, a black frame) and
  bans (invented claims, generated imagery) are not judgement calls.
- Verify by looking at the render, in batches: pull stills at the moments that matter, `look` at them in one round,
  fix everything they show, then look again. Don't take a still per tweak.
- Commit. With the floor green, spend the piece on its idea. When torn between safe and committed, commit, then make
  the committed thing clear.
