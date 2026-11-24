---
title: "raea clothes"
summary: "Clothes for SOMA's body from words: make sews a garment from a sewing pattern (GarmentCode, MIT), cut to SOMA's measurements and draped by raea's own cloth, as assets/clothes/NAME.json and its .glb; cover lays one on the skin by procedure; wear puts garments on a body."
order: 106
sources: ["engine/cli/src/cli.rs"]
---

Clothes for SOMA's body from words: make sews a garment from a sewing pattern (GarmentCode, MIT), cut to SOMA's measurements and draped by raea's own cloth, as assets/clothes/NAME.json and its .glb; cover lays one on the skin by procedure; wear puts garments on a body.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea clothes [make NAME ["WHAT IT IS"] [--plain] [--set PATH=VALUE,...] [--colour #RRGGBB] | settings | cover NAME --parts PART[:SHARE],... --colour #RRGGBB [--stand MM] [--over UNDER] [--hard [--sole MM] [--sole-colour #RRGGBB] [--panels #RRGGBB,...]] | wear OUT_DIR GARMENT... [--body DIR] [--head HEAD_DIR]]   (clothes for SOMA's body from words: make sews a garment: a sewing pattern (GarmentCode, MIT) from the words, or the plain base with --plain, its settings changed by --set PATH=VALUE (settings lists them and what each may be), cut to SOMA's measurements, draped by raea's own cloth and baked, as assets/clothes/NAME.json and its .glb, in --colour or the palette's next; the first make sets up GarmentCode and its Python in raea's models folder; cover makes one by procedure in the same file, no picture: the skin of the parts named (hips, waist, torso, chest, neck, head, upperArm, foreArm, hand, thigh, shin, foot, toes; a limb's SHARE is how far along it from its top), in one colour, MM off the skin (6 unless said), --over a garment already made as the one under it; wear puts garments on a body, innermost first (each GARMENT a name or its .json): OUT_DIR/NAME.glb each, with OUT_DIR/body.glb the body without the skin wholly under cloth; --body DIR another SOMA body folder (a headless one for a head part); --head HEAD_DIR the head that joins it (head.json), whose skin the body's is graded to at the collar; alone, the project's garments. The first use fetches SOMA's body (NVlabs/SOMA-X, Apache-2.0) into raea's models folder and needs Blender: raea setup blender)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--body` | `DIR` | With `wear`: dress this SOMA body folder (body.glb, skeleton.json) instead of SOMA's own, such as a headless body that a generated head joins at the collar. |
| `--colour` | `#RRGGBB` | The garment's one colour. With `make` it is optional: one is picked from the name unless said. |
| `--hard` | – | With `cover`: footwear, made as a hard piece. Each foot gets its own shape: a rounded last around the foot, so the toes and the foot's small forms go under it. Underneath is a flat sole, and the top over the leg is left open as the collar. It is skinned to the foot like the skin it covers, and it does not swing. |
| `--head` | `HEAD_DIR` | With `wear`: the head folder (head.json, head.glb) that joins a headless body at the collar. The body's skin takes the head's neck colour and is graded to the head's own skin just under the collar, so no step of colour shows at the join. |
| `--over` | `UNDER` | With `cover`: lay it over one garment made by `cover`, a layer's thickness off it. |
| `--panels` | `#RRGGBB,...` | With `cover --hard`: the colours of up to three panels, in this order: a toe cap, a heel, and a band along each side. |
| `--parts` | `PART` | With `cover`: the body's parts the garment covers, comma-separated; a limb takes a share, how far along it from its top (`foreArm:0.5` is the half nearest the elbow). |
| `--plain` | – | With `make`: start from the plain base (a short-sleeved top) instead of a pattern guessed from words. The default when no words are given. |
| `--set` | `PATH=VALUE,...` | With `make`: change settings of the garment's pattern by path, comma-separated (`sleeve.length=0.9,meta.bottom=Pants`). A choice must be one of its options and a number within its range; `raea clothes settings` lists them. |
| `--sole` | `MM` | With `cover --hard`: the sole's thickness in millimetres (10 unless said, at most 60). Most of it is painted up the side; the bottom sits at most 8 mm below the bare foot, so the person does not sink into the floor. |
| `--sole-colour` | `#RRGGBB` | With `cover --hard`: the sole's colour (an off-white unless said). |
| `--stand` | `MM` | With `cover`: how far the cloth stands off the skin, in millimetres (6 unless said). |

<!-- generated:end -->

## An outfit

```sh
raea clothes make tee --plain --set shirt.length=1.5 --colour "#e8e2d6"
raea clothes make trousers --set meta.upper=none,meta.bottom=Pants,meta.wb=StraightWB,pants.length=0.8 --colour "#2f3b52"
raea clothes wear assets/characters/dressed trousers tee
```

`make` sews a garment: a sewing pattern (flat pieces joined by seams) cut to SOMA's measurements, draped on the body by gravity and baked to a skinned glb. It costs nothing and takes about 20 seconds. The pattern comes from the plain base (a short-sleeved top) with `--plain` or when no words are given, and each `--set PATH=VALUE` changes one of its settings; `raea clothes settings` lists them all with what each may be. Given words (`raea clothes make hoodie "a hooded sweatshirt"`), the pattern is the garment model's first guess from them; the command says what it made, and `--set` corrects it.

The first use sets up what sewing runs on in raea's models folder (`garments/`): GarmentCode's pattern components (MIT) at a fixed commit, a Python 3.11 environment made by `uv`, and SOMA's body as a distance grid for the drape.

A sewn garment is `assets/clothes/NAME.json` with its files in `assets/clothes/NAME/`: the glb, the design, the flat pattern and four pictures of it on the mannequin. Look at the pictures before you wear it.

`cover` lays a garment on the skin by procedure, for what is skin-tight; it can't hang free. `wear` writes a body folder with each garment's glb and the body without the skin that is wholly under cloth; with `--head`, the body's skin is graded to the head at the collar.
