<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: critique's two isolated assessments, synthesis and snapshot, rewritten for video. See NOTICE. -->
# Stage: critique

Judge the piece as it renders, not as you meant it. Two passes that don't see each other, then one list of fixes.

## Pass A: the design review (judgement first)
Before reading any lint output, watch the piece as a viewer would: `raea sheet` for the whole, then stills of the
first 1.5 s, every designed moment, every cut you're unsure of and the end, looked at together. When void hands you
a creative director's notes on the whole render, they are evidence for this pass. Then:
1. **Specificity verdict, first.** Cover the logo and the name: could another brand post this? Could you guess its
   look from the category? Answer in one sentence before scoring anything.
2. **Score the ten heuristics** in `rubric` (0-4, n/a where the mode makes one meaningless).
3. **Walk it as the four viewers**, plus the mode's own, and note the moment each one leaves or stops believing.
4. **Hold it against DIRECTION.md.** Did the build keep the thesis, the world, the first 1.5 s, the sound? A
   direction quietly softened during the build is a finding.
5. Name two or three things that work, so the fixes don't break them.

## Pass B: the measurements
Run `raea check --full` on the same render (the floor's measurements, the model checks and the brief's checks). List what it reports,
with frames. Read B only after A is written, so the numbers don't decide what you
think of the piece.

## Synthesis
Merge both into one list of issues, each with its moment, what's wrong as the viewer meets it, its severity
(`rubric`: P0-P3) and the playbook that fixes it (`refine/pace`, `refine/typeset`, ..., or back to
`stages/direct` or `stages/strategy` when the issue is the idea itself). Where A and B disagree, say which you
trust and why. Note what each pass caught that the other missed.

Write it, with the heuristic scores and the total, to `critique/<n>.md` (n counts up), so the next pass can see the
trend. A score that drops after a fix means the fix broke something.

## Then
- Total below 50, or the issue is the idea: return to strategy or direct. Don't polish a direction you'd discard.
- Otherwise: polish, taking the P0s and P1s.
- At most three critique rounds per piece; stop when a round resolves nothing new.

next: stages/polish
