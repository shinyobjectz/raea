<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: new-work's build principles, rewritten for video. See NOTICE. -->
# Stage: build

Build the direction you wrote, not a safer version of it. The procedure says which commands make the moves; this
says the order and what to hold. How to write the TSX is in `raea craft motion` and its rules.

## Order
1. **Cut first.** The brief's lines, in order, cut clean (`raea script edit.brief`, or `raea cut` by hand). The
   edit's length is the cut's. Run `raea check` before designing anything on top: design on a broken cut is
   wasted.
2. **The hero frame.** Build the first 1.5 s fully, exactly as DIRECTION.md's FIRST 1.5 S describes: picture,
   type, motion and sound. Then pull a still of it and `look`. This frame is your comp: if it doesn't stop you,
   nothing after it will help. Fix it until it holds before building the rest.
3. **The beats, in order.** Each designed moment built in the direction's world, looked at when it's done (stills
   of the frame where it's fully in and of its entrance, looked at together).
4. **Sound** as DIRECTION.md's SOUND says (`refine/sound` for the mechanics of a good mix).
5. **The floor**, in one batch: `raea check`, then stills and `look`. Fix what it reports, or record a kept
   exception in DIRECTION.md.

## Hold while building
- **The opening is a thesis, not a title card.** Show the mechanism or the result at once, at the scale it has in
  life. Memory test: if someone scrolled away after 1.5 s, what would they describe? If the answer is a mood, the
  opening hasn't committed.
- **Prove, don't claim.** Show the product doing its job, the number with its source, the real result. Specifics a
  competitor couldn't copy-paste.
- **Make the material; don't fill with chrome.** Gradients, glass, icon tiles and empty pills where content
  belongs are the gap showing. Author the content at full quality: the words, the diagram, the frame.
- **Every atom in the world.** Captions, the CTA, a source credit, the end frame: all set in the direction's faces,
  roles and motion. A stock-looking part inside a committed direction is a lapse.
- **One grammar, varied.** Vary scale, density, motion and quiet across beats inside one system; a dense beat earns
  a quiet one; the piece ends anchored by a real close.
- **Timed to the words.** Parts arrive on the words that say them (`word("phrase")`, `useWords()`, `on:` in a
  brief's notes); exits make room before the next thing needs the eye.
- **Real pictures only.** Footage, stock in the space, or what you build. Never generated imagery.
- **The person's edits stand.** Read edit.tsx and its components before changing them; keep changes you didn't
  make.

## Done when
Every beat built, the floor run, and one full round of stills over the hook, each designed moment and the end shows
nothing you'd fix. Then critique.

next: stages/critique
