<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0; rewritten for short-form video. See NOTICE. -->
# The craft floor

The floor is mechanics, never direction: it says what breaks a piece on a phone, not what the piece should look
like. Check all of it on one render, not in separate trips, and before you present, go through the defects and
bans on the last render and fix each one you find. Three kinds of item:
- **Defects**: always fixed. Nobody earns a clipped word.
- **Lints**: inform. Each names a risk and how to see it. Keep a flagged choice only when the idea earns it, and
  write the reason in DIRECTION.md in one line.
- **Bans**: truth and policy. No idea earns them back.

## Defects
- **Clipped words and bad joins.** No cut lands inside a word or on a breath; no stray frame flashes beside a join;
  no frozen, hung or black frames; no dead air left inside the talk (`raea check` measures all four).
- **Unreadable text.** Text that can't be read on a phone at arm's length is not on screen. Captions and any text
  the viewer must read: at least 60 px type at 1080 wide (scale with the frame's width), and at least 3:1 contrast
  against the pixels actually behind each glyph, not against the colour you imagined. Small print (legal, a source
  credit) at least 32 px and held long enough to read.
- **Text cut off.** No word runs off the frame, off its card or into the next line. `raea check` measures
  this and the stills show it.
- **Wrong length.** The piece is within the length asked for.
- **Loudness.** Master at -14 LUFS integrated (±1) and true peak at or below -1 dBTP (`<Mix>`), with the voice
  intelligible over everything else. Feeds turn loud masters down and leave quiet ones quiet. Changing it needs the
  person's say-so.

- **A name misspelled.** Brand, product and people's names are spelled as BRAND.md (or the brief) has them, never as
  the transcript heard them: speech-to-text mishears names ("Alpha Line" for Alpha Lion). Check every caption, card
  and end card against BRAND.md before you call it done.

## Lints
- **Text over the face.** Text or a card covering eyes, nose or mouth while the person talks hides the most-watched
  part of the frame (`raea check` measures it; a part inside a `<Region>` keeps clear). Default: keep it clear,
  using the face's confirmed box.
  An idea can earn an overlap (a censor-bar gag, a sticker on a shoulder, a reaction frame), never by accident.
- **Platform UI zones.** The app's own buttons, caption and handle cover parts of the frame. Keep what must be read
  out of them for the shape being made (1080-wide frames; scale for others):
  - 9:16 (1080x1920), platform unknown or several: top 220, bottom 420, right 140, left 60 (the union; `<Region>`
    keeps to it). One named platform: TikTok top 108, bottom 320, right 120, left 60; Reels top 210, bottom 310,
    right 84; Shorts top 120, bottom 300, right 96.
  - 4:5 (1080x1350, feed): top 110, bottom 170, sides 60.
  - 1:1 (1080x1080, feed): top 70, bottom 120, sides 60.
  - 16:9 (1920x1080, YouTube, landscape): top 120, bottom 120 (player controls), sides 60.
  A box's `top` is its upper edge: a line placed near a zone's edge may already reach into it. These zones are where
  text can't go; they say nothing about where it should go. Placement is a design decision (`refine/layout`).
- **Caption collisions.** Captions never sit on a card, a logo, burned-in text in the footage or the product; they
  don't jump between positions from cut to cut without a reason the viewer can feel.
- **Reading time.** About 0.3 s per word on screen and 1 s at least for anything the viewer must read. A number or
  a claim gets time to land before the next thing replaces it.
- **The hook is late.** Something meant to stop the thumb is on screen and in the ear by 0.5 s.
- **The ask is missing or unworkable.** When the mode has an ask, it is on screen long enough to act on, in its
  working form (the offer, the code, the handle), inside the zones.
- **Template sameness.** The piece could be swapped with the last few pieces, or with any brand's (same angle, same
  structure, same signature move, same entrance on every part). Within one brand a shared look is the brand: vary
  the angle, structure and signature move. Across brands, vary the look too. Fix by re-rolling the direction.
- **One move everywhere.** The same entrance, the same zoom, the same whoosh on every beat flattens all of them.
- **Faces by habit.** Faces models reach for without looking (Inter, Montserrat, Poppins, Outfit, Playfair
  Display as a display voice; a heavy condensed sans in uppercase as the only caption voice) are fine when chosen
  for a reason the piece states, and a lint when they are the default.
- **The four generated looks.** Warm cream ground with a serif and a terracotta accent; near-black with one neon
  accent and glow; hairline editorial with italic serif and tracked mono labels; flat saturated ink with black
  boxes and hard offset shadows. Each is legitimate when the brief calls for it. Landing in one when the brief left
  the look free means you didn't decide. If someone could guess the look from the category alone, rework it.

## Refuse unless the idea earns it
These are the category's defaults. Reaching for one when the axis is free means you weren't deciding.
- The template ad: white bold captions with one yellow word, emoji pops, a zoom on every sentence, a whoosh on
  every entrance.
- A big-number card as the automatic form of proof; a number with nothing behind it.
- Glow, gradient text, glass panels over footage, drop shadows used as decoration instead of separation.
- A lone title floating in empty space; a pill or card that exists because the beat felt empty.
- Bounce and elastic easing by reflex.
- Copy that sounds like an ad wrote it: buzzwords, "Not X. Y." cadences, aphorisms, a label above a headline.
- Emoji or unicode glyphs standing in for drawn icons.

## Bans
- **Invented claims.** Every number, result, price, testimonial, customer name or comparison on screen or in the
  voice is in BRAND.md's allowed claims or said in the footage. Absent evidence stays absent; don't round up, don't
  imply. When the piece needs proof that doesn't exist, build the best piece without it and say what would help.
- **Generated imagery.** No AI-generated images or video, ever. Pictures come from the person's footage, stock in
  the object's media, or are built (components, drawn shapes, 3D). Generated sound is allowed by policy: TokenSynth
  instruments for music, SAO for foley (both local), Fish for narration over the picture.
- **Redrawn logos.** Use the real logo files; never redraw or invent one.
