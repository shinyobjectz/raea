<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: polish, rewritten for video. See NOTICE. -->
# Stage: polish

Polish refines the piece in its own direction. It never hides a redesign: if the direction itself is wrong, say so
and go back to `stages/direct`.

## 1. Take the list
Read the latest `critique/<n>.md`. Take every P0 and P1, and P2s where the pass has room. A lint result is defect
evidence, not proof of quality: still look at the frames.

## 2. Classify each drift before fixing it
- **from BRAND.md**: a colour, face, claim or logo use the brand doesn't allow;
- **from DIRECTION.md**: a part, move or sound that left the direction's world;
- **a one-off**: a part that should reuse an existing component with new props instead of being its own thing;
- **a local defect**: simply unfinished.
Fix the cause at the narrowest level: a component's props, one class, one line of the brief.

## 3. Fix in this order
1. Broken: floor defects (clipped words, black or frozen frames, unreadable text, wrong length, loudness) and bans
   (a claim without evidence).
2. Missing: what the mode needs and the piece lacks (the hook by 0.5 s, the proof at the claim, the ask, captions
   when the piece needs them).
3. Drift from BRAND.md and DIRECTION.md.
4. Motion and sound: timing to the words, entrances and exits, the mix.
5. Cleanup: frames of timing, alignment, a colour step, unused components and imports.
Don't perfect one beat while the rest sit below it.

## 4. Verify
One batched round: `raea check`, then stills of the hook, each designed moment, each fixed moment and the end,
looked at together.
Then update DIRECTION.md if the build legitimately moved (it must describe what was built), and close the critique
by noting in `critique/<n>.md` which issues were fixed.

The piece is finished when the floor is green (or its exceptions are written down), the P0s and P1s are fixed, and
DIRECTION.md is true. Then tell the person what changed, from their side.
