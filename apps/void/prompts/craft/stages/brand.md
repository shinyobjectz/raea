<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: init's PRODUCT.md, rewritten as BRAND.md for video. See NOTICE. -->
# Stage: brand

BRAND.md holds the brand's truth: what it offers, to whom, what it may claim, how it sounds, and what its colours
and faces are for. It never holds a piece's look; that's DIRECTION.md, per piece. Read it before strategy, cite it,
never restate it into the brief.

`raea brand DOMAIN|NAME` is brand ingestion: from the brand's site it writes the object's brand kit
(`brand/brand.css`, `brand/brand.json`, logos in `media/brand/`) and `BRAND.md`. Colours there are picked by role
from the brand's own colours only; a role no brand colour fills legibly is left empty, and filling it is your
decision for the piece.

## When BRAND.md exists
Read it whole. Note for this piece:
- the claims you may use and the evidence behind each, and what is absent (absences are facts too: no
  testimonials means none on screen);
- the voice (its three words and its banned phrases) for every word you put on screen;
- the palette and faces, which you'll turn into roles below;
- the logos, which mode is for which ground, and their minimum size.

## When it's missing or thin
Write a provisional one with the shell, marked `Status: provisional` on its first line, from what you can see: the
kit `raea brand` found (name, colours, logos), the person's request, and the footage's transcripts. Write only what
those sources say. Don't interview the person about aesthetics; ask one question only when a claim or the offer is
unclear and the piece depends on it.

```
# BRAND.md
Status: provisional | confirmed
Offer: what's sold or asked, in one line
Audience: who, and the scene they watch in (where, when, in what state)
Positioning: the one claim a competitor couldn't truthfully copy
Allowed claims:
- claim | evidence | source (BRAND kit, the footage at take/time, the brief)
Absent: proof the brand doesn't have (no reviews, no price, no clinical data ...)
Voice: three words; phrases it never uses
Palette: each colour as given, with what it's for (see roles below)
Type: the brand's faces, and the available face used for each when the brand's own isn't installed
Logos: files in media/brand/, which for light grounds and which for dark, smallest legible size
People: who appears, how to refer to them
Do / Don't: commitments the brand has made (a colour never used, a word always used)
Platforms: where it runs, in which shapes
```

## Palette roles
Brand colours arrive as swatches. A piece needs roles. Decide each role's job before its colour:
- **ground**: fields that own regions (a card, a slide, an end frame), if the piece uses fields at all;
- **ink**: text on that ground and text over footage (with what makes it legible over pixels: a shadow, a scrim,
  a plate);
- **accent**: the rare colour that marks what matters (the payoff word, the proof, the ask). Its force comes from
  rarity; spent everywhere it marks nothing;
- **support**: secondary information, dividers, labels.
A brand with one colour gets its roles from tints and shades of it plus a neutral drawn toward its hue. A brand whose
colours fail contrast over its likely footage gets a plate or scrim role, not a different colour. Record the roles
in BRAND.md under Palette when they're durable for the brand; per-piece choices go in DIRECTION.md.

## Type
The brand's faces win when they're available (`raea craft motion/brand`).
When one isn't, choose the available face nearest in the trait that carries the brand's character (width,
weight, contrast, geometric or humanist, how it sets in caps) and write the substitution down. A brand with no
faces of its own leaves type to each piece's direction.

next: stages/strategy
