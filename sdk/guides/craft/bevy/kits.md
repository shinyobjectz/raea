# Kits: props in one style, at real size, made and checked as a set

A kit is a set of objects that belong together (a 1970s flight desk's phone, clock, ashtray and lamp) and land as
components any project can import. It holds together because every reference comes from the same style images and
paragraph, every model is picked on clay from two generators, and the whole set is checked on one sheet before
anything costs much. This is AnyKit's loop (MIT), each step on a `raea make` verb; `raea make status` holds you to it
and the Stop hook won't let you end before it says done.

## What to do

1. **The plan.** `make: kit({ references: ["media/style.png"], style, objects, dials })` on brief.ts (`raea/brief`).
   - `style` is one paragraph every reference carries: era, palette, materials, wear, and one fictional maker
     ("Aurora Instruments"). Never a real brand.
   - Each object: `id` (snake_case), `label`, one size (`h` metres tall, `dims` [w, d, h] for boxy things, `max` for
     things lying flat), `prompt` (one object, its condition and materials), `hero: true` for the ones seen close.
   - `raea spec brief.ts --as make` prints the plan and what it still lacks.
2. **References, then the reference sheet.** `raea make refs --dry` quotes it; `raea make refs` makes each object's
   reference from the style images, in the style paragraph, with the scan template (one object, whole, plain grey
   ground, even light), then writes `review/refs.png` (style images first, then every reference with its size).
   - Look at the sheet as a set. One that's off (another era, another palette, two objects, a logo) is regenerated
     now: `raea make refs --only ID`. Ask it: the command the sheet step prints (`make.refs.*`).
3. **Both generators.** `raea make gen --dry` quotes it; `raea make gen` sends each reference to Tripo H3.1 (150k
   faces for a hero, 60k for a prop) and Meshy on fal, into `out/kit/gen/ID/`.
4. **Pick on clay.** `raea make pick-sheet` writes `review/picks.png`: the reference, then each model fitted to its
   real size, in colour and in clay. Judge on the clay: fused parts, blobs where there should be edges, and fidelity
   to the reference's shape. `raea make pick ID tripo|meshy "why"` records the pick and its reason.
5. **Prep.** `raea make prep` fits each pick to its real size (front toward +z, bottom-centred, metres) with
   `raea gen check`, makes a quarter-triangle `_lod1` beside it (`blender/decimate.py`), and writes a `.glb.json`
   sidecar for each saying how it was made (generator, why, prompt, reference, style).
6. **Land it.** `raea make kit --name KIT` puts the kit in the workspace's `shared/components/KIT/` (else the
   project's `components/KIT/`): one component per object (`RotaryPhone.tsx`, importing its model and LOD from
   beside it, `lod` for when it's small on screen), `lineup.tsx` (every piece at its real size beside a 1 m bar),
   `cover.png`, `thumbs/` and `kit.json`. Import it as `@shared/components/KIT/RotaryPhone`.
7. **Review and judge.** `raea render shared/components/KIT/lineup.tsx --review review/rN`, then
   `raea judge review/rN --kind kit --by claude --brief brief.ts --json`. The kit rubric: cohesion, reference
   fidelity, the brief, mesh (clay), texture, assembly (clay), scale (clay), labels. Fix its blocking defects or
   answer them in NOTES.md, and keep going until `raea make status` says done.

Cost: a six-piece kit is about $8-10 (references about $0.25 each, Tripo H3.1 and Meshy about $0.60-0.80 each).
`--dry` on refs and gen says the price first; ask before going over $10.

## Why

- One style paragraph and the same style images in every reference is what keeps a kit of one world. References
  made one at a time from text alone drift in era and palette, and an outlier caught on the sheet costs a picture,
  not a model.
- Generators fail differently: Tripo fuses a handset to its cradle, Meshy softens numerals. Two models and a pick on
  clay beat any one, and the reason written down is what the next kit learns from.
- Fitting to real size on the way in is what makes a kit drop into a scene without nudging.

## Common mistakes

- Picking on the colour render: texture hides a blob. Pick on the clay.
- A real brand in a prompt or the style ("Bakelite by Western Electric"). Name the one fictional maker.
- Skipping the reference sheet because each reference looks fine alone: judge them together.
- Landing a kit before `raea make prep`: `raea make kit` names what isn't prepped rather than landing half.
