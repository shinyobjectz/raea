<!-- Adapted from Impeccable (Paul Bakaus, pbakaus/impeccable), Apache 2.0: layout, rewritten for frames with faces and platform UI. See NOTICE. -->
# Refine: layout

Layout in a video is where things sit in the frame relative to the face, the action and the app's UI, and how that
holds across cuts and shapes. Diagnose the structure before moving boxes.

## Assess (with frames)
- **The squint test.** Blur the frame: can you still tell what leads, what supports, and where the face is?
- **The face and the action.** Where is the face on each clip (its confirmed box, from perception)? Where does the
  product or the action happen? Text belongs where it doesn't compete with them, unless the idea says otherwise.
- **The zones.** Is everything that must be read out of the platform's UI zones for this shape (the floor)?
- **Consistency across cuts.** Do captions and recurring parts stay in one place from cut to cut, or jump?
- **Collisions.** Captions against cards, logos, burned-in text, the product.
- **Room.** Is a part crammed into a band too small for it, or floating in a field too big?

## Set the spatial thesis
Name: what leads in each beat, where the eye goes first and next, which band each role lives in for this piece (from
the face and the zones, not from habit), and how that changes when the framing changes. Placement comes from the
footage: a face high in frame leaves room below; a wide shot leaves room beside. `<Region name="auto">` (and
the brief's `place-*`) find clear bands from the face; choose among them by what the beat needs.

## Apply
- Group by meaning: a statement and its source sit together; unrelated things separate.
- One band per role across cuts, moved only when the framing forces it, and then with a transition.
- When the frame has no room (a crop so tight the face fills it), change the footage (reframe it, punch in less) or the
  design (fewer words, smaller part), never force a part over the face.
- **Other shapes.** Reformatting 9:16 to 4:5, 1:1 or 16:9 is relayout, not scaling: re-read the face on each clip
  at the new aspect (`raea frame`), recompute bands from the new zones, and shorten or reflow parts that no longer
  fit. 16:9 usually wants text beside the speaker, not above.

## Verify
The squint test holds; `raea check` clean (or the exception written down); captions don't jump; every
shape made has been looked at, not assumed.

next: stages/polish
