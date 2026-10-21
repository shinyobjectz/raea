<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: shape and new-work's rut and grounded candidates, rewritten as creative strategy for video. See NOTICE. -->
# Stage: strategy

Strategy decides what the piece says, to whom, and in what shape, before anything is designed. It lives in the
piece's brief: the decisions at its top, a section per beat, and the lines quoted from the takes. No colours or
looks yet; those belong to direct and build.

A piece made without footage (a game, a deck, a motion piece) has no takes to quote: its strategy is the mode's
success, the one idea, the beats or rules, and the words you write. Skip what follows about takes, and write it in
DIRECTION.md instead of a brief, since `raea script` compiles only lines said in a take.

## 1. Read what's true
- The request: what was asked, the shape and length, where it runs.
- BRAND.md: the offer, the allowed claims and what's absent, the voice.
- Every take's transcript (`raea media`). Mark the lines that are strong as
  spoken: specific, surprising, in the person's own voice, delivered with energy. The footage decides what the piece
  can say; an angle no take says is fiction.
- Choose the mode (ROUTER) and load its file.

## 2. Name the rut
In two sentences: the piece this category always posts (its usual opening, structure and ask), and its predictable
opposite. Both are the rut. Write them down so you can see when you drift back into one.

## 3. Draft angles
Write five to seven angles. Each is: the one thing the viewer will believe or feel, why this viewer cares (from
their scene in BRAND.md), and the lines in the takes that carry it. Draw them from at least three different
sources, for example:
- a line the person said that no script would have written;
- a product truth only this brand can claim (BRAND.md's positioning and allowed claims);
- a tension or ritual in the audience's life (what they do the minute before they need this);
- a form from the audience's culture the idea could borrow (a confession, a review, a tutorial, a rant, a
  countdown, a myth and its truth);
- a contrast: before and after, them and us, what people think and what is so.
At most one angle may be the literal reading of the brief. Drop near-duplicates and anything that needs a claim
BRAND.md doesn't allow.

## 4. Choose
Pick the angle with the strongest real material: the best lines, the clearest proof the footage holds, the most
specific to this brand. Not the most common angle and not the safest. When the person or the brief pinned an angle,
it wins. Write one line on what the chosen angle refuses (the rut it avoids).

## 5. The hook
Pick the kind of opening the angle and the footage support, and the exact line or picture that does it:
- a confession or admission ("I was wrong about...");
- a pattern break: a picture or sound the feed doesn't expect;
- the claim with its proof on screen in the same second;
- the demo first: the result happening before any context;
- the viewer's own question, in their words;
- in the middle of the action, explained later;
- the result first, then how;
- a contrarian line against what the category says;
- a promise of a list, a ranking, a number of things;
- a callout to a specific viewer ("if you ...").
The hook must be in the takes (or built from real material). Two candidates are fine; choose by which is stronger
muted at 1.5 s.

## 6. The shape is a choice
Pick a structure because the angle needs it, never because it's the usual one. Some shapes: problem, turn, proof,
ask; result first, then the how; one continuous demonstration; a list that escalates; a story with a setup, a
complication and a resolution; myth, then truth; before and after; a question answered step by step; a loop whose
end returns to its opening so the second watch starts by itself. Count the beats from the length: a short piece
holds fewer beats than you think. Say in each beat's synopsis what it does for the viewer.

## 7. Write the brief
Write the piece's brief with the shell. It holds the decisions (title, format and length, mode, angle, hook, what it
refuses), then one section per beat in your shape's order: a name of your own, a line on what the beat does for the
viewer, and the lines kept, each copied word for word from the transcript with the take it comes from. Lines are
cuts: a changed word is a line no take says. `raea script` compiles the piece's brief into edit.tsx and prints a
sheet (every line timed, the total against the length, anything it couldn't read); fix the brief until the sheet is
clean. Or build from the brief by hand with `raea cut`.

next: stages/direct
