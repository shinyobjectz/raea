<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: new-work's roll, raises, colour strategy, face choice, calibration and Direction contract, rewritten for video. See NOTICE. -->
# Stage: direct

Direct chooses this piece's visual and sonic direction and writes it to `DIRECTION.md`. The direction is chosen per
piece, from this brand's truth and this piece's angle, by a roll among grounded candidates. It is never a default
and never the last piece's look reused because it worked.

## 1. What's already decided
- A direction the person or the brief pinned (a look, a reference, "like the last one") wins over everything here.
  Translate it faithfully; skip the roll.
- BRAND.md's commitments (palette roles, faces, logo rules, do/don't) bind every candidate. Within one brand a
  shared look is the brand: what varies piece to piece is the form, the composition, the signature move, the
  sound. Across brands the whole look varies.
- Earlier pieces: if the object or brand has a LOOKBOOK.md or earlier DIRECTION.md files, read them. A candidate
  that repeats the last pieces' form and signature move is out.

## 2. Five to seven candidates
From the angle and the audience's world (BRAND.md's audience scene), list five to seven directions. Each one is a
form the audience knows by heart that can carry this angle, with one line on why it fits and one on how it looks
and sounds (ground and main colours as roles, the type voice, the one signature that makes it recognisable, the
sound). Rules for the list:
- Ground them in the audience's culture: its media, rituals, places, documents, broadcasts, packaging. Include
  forms from before phones.
- At most one candidate may be the literal metaphor of the product or the brief.
- At most one may be the category's own visual habit played straight.
- When more than three come from one family (all screens, all print, all broadcast), dig further.
- Add one or two challengers from forms far from the category, so the roll has range. Some forms to borrow a
  grammar from: a receipt, a sports broadcast, a field guide, an infomercial played straight, transit signage, a
  museum label, a group chat, a weather report, a lab notebook, a recipe card, a nature documentary, a teardown,
  a departures board, a ticket stub, a hand-painted shop sign, a court sketch, a VHS box, a zine, a karaoke screen,
  a pharmacy label, a scoreboard, a cooking-show countdown. A borrowed form supplies structure and grammar; the
  brand supplies every fact and colour; clarity wins every conflict. Never a costume.
- Every candidate must be buildable from the real footage and TSX components, and truthful: a candidate that
  needs a claim or an image the brand doesn't have is replaced before the roll.

## 3. Roll, don't rank
Number the candidates and roll for the lead in the shell: `echo $((RANDOM % N + 1))` (N candidates). The roll
assigns the direction. Ranking is where every piece drifts to the most common answer; the roll is what keeps pieces
apart. Then weigh each other candidate against the rolled one on two questions only:
would this viewer recognise themselves in it, and is the product clearer in it?
- A challenger that beats the rolled one on both replaces it (say so, and why).
- Otherwise it donates one discipline the rolled direction lacks (a palette's total commitment, a rhythm's
  courage, a form's structural honesty), written into DIRECTION.md as a named raise. A donation is a discipline,
  never the challenger's clothes.
Re-roll only on facts (the rolled direction can't carry the claim, or the footage can't support it), never taste.

## 4. Commit the world
- **Colour strategy first, colours second.** Restrained (neutrals plus one accent), committed (one colour owns
  much of the frame), full (three or four named roles), or drenched (the frame is the colour). Persuasion and
  organic hooks can take the bolder strategies; explainers and demos usually want restraint around the content.
  Then assign BRAND.md's colours to roles (`stages/brand`) and add only what the roles need.
- **Light or dark from the scene.** One sentence on where and when the viewer watches; let it decide.
- **Faces like objects from the form's world.** One display voice and one text voice at most, from the brand or
  from the faces the renderer has, chosen for a trait the form needs. A face you'd pick for any piece is the
  floor's lint; name why this one.
- **The caption treatment** if there are captions: how they're set, broken and lit, so they belong to this world.
- **The signature move**: one device, used where it means something.
- **Sound**: the bed's character (or silence), the one family of foley, where the quiet goes.
- **Self-check** against the floor's four generated looks and the category guess: if the look could be guessed
  from the category, rework it before building.

## 5. Write DIRECTION.md
About 150 words, written with the shell. If a block reads like a mood, the direction isn't decided yet.
```
MODE: <the mode>
THESIS: the one idea this piece owns, and the category habit it refuses
OWN-WORLD: palette roles (the brand colour in each), display and text faces, caption treatment, signature move;
  specific enough to recognise a frame with every word removed
STORY: what the viewer understands, believes, does
FIRST 1.5 S: the exact opening frame: what is where, at what scale, what is heard
FORM: the form, candidate k of N, the roll's number, raises (named for their donors)
SOUND: bed, foley family, silences, master
EXCEPTIONS: any floor lint this piece keeps on purpose, and why (one line each)
FINISH: unreviewed is unfinished; this piece ends with a critique pass, the floor green, and DIRECTION.md true to
  what was built
```
DIRECTION.md never enters the piece: no direction text rendered, no labels from it on screen.

next: stages/build
