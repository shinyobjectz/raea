<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: the Read mode, nearly as written, for slides. See NOTICE. -->
# Mode: deck

Pages to be presented or sent: a pitch, a report, a lesson. A 16:9 composition with one `<Sequence>` per page,
exported as stills, sometimes played as a video. The stage plays a deck live, full frame, at the window's size: lay pages out from
`useVideoConfig()` so they fill it, and let the arrow keys step through pages (`useInput()`, with `live: true` while
someone is presenting).

## The viewer succeeds when
They remember three points a day later and know what they're being asked to decide or do. A deck that dazzles and
leaves no takeaway failed; so does one that is a document pasted onto slides.

## What ambition looks like
- **One message per page.** The page's headline states its point as a sentence, not a topic label. Everything else
  on the page supports that sentence or leaves.
- **A document of its kind, framed by the world.** The direction owns the frame: the title pages, the section
  openers, the ground, how numbers and charts are set, one signature device (a running mark, a colour rail, a
  numbering scheme that carries meaning). The reading inside stays calm: real contrast, generous space, a clear
  hierarchy of headline, support and source.
- **Evidence set well.** Charts drawn from the real numbers with the conclusion in the headline; quotes with who
  said them; one decisive image over five decorative ones.
- **A real close.** The ask, the decision, the next step: stated, on its own page.

## The rut to name
Title-plus-bullets on every page; same-size cards of icon, heading and text; a hero number with a small label on
half the pages. The opposite rut: a poster per page with no reading order.

## Watch for
- Density: a page read in a room holds less than a page read alone. Decide which this is.
- Consistency: same-role type identical on every page; the system's spacing held.
- Export: check every page as a still (`raea still` at each page), not only the first.

Rubric weight: 2, 3, 6, 7 and 9 decide it; 1 becomes the title page; 5 is the sequence of pages; 8 is n/a unless
it plays as video. Add the decision maker as a fifth viewer.
