---
title: "raea check"
summary: "Check a composition's text in a second, a project against every rule it holds, or an export against the floor: text, contrast, safe areas, freezes, flashing, cues and loudness."
order: 42
sources: ["apps/check/src/lib.rs", "apps/check/src/quick.rs", "apps/direction/src/check.rs", "sdk/raea/src/check.ts"]
---

`raea check` has two speeds. On a composition with `--quick`, it renders three stills and measures the text:
clipped, cramped, inside the safe areas, large enough to read, and what the stills' render found (3D parts clipping).
The stills are a quarter in, the middle and the last frame of the length a render of frame 0 reports, however
`durationInFrames` is written, and the report names them (`ok looked at frames 75, 150 and 299 of 300`).
That takes about a second, so run it after every save. On an exported video, it measures everything the floor asks
of a finished piece. Each finding is an error, a warn or a note (raea's one finding, as `raea scene lint` and a
render's manifest have it), says on which frames and how to fix it, and the command exits 1 while an error stands.
A check of a file in a raea project keeps its findings in the project's record (`.raea/record/`), beside its
renders', which `raea findings` and `raea next` read.

```sh
raea check piece.tsx --quick --platform tiktok
raea render piece.tsx -o piece.mp4 --manifest render.json
raea check piece.mp4 --manifest render.json --words words.json --platform tiktok -o check.json
```

What a full check can see depends on what you give it. The file alone gives loudness, true peak, sound that stops
dead, frozen stretches and blotching in dark areas. The render's manifest adds text clipped off the frame, text under
the platform's buttons, captions held too briefly and caption contrast. The transcript adds cues away from their
word and music fighting the voice; the perception adds text over a face. A taste file adds the person's own rules,
each an error when broken, since their rule beats the floor's default.

On a render made in a raea project, the check also uses the project's record. A finding an earlier export of the
same composition had at the same moment ends `[also in …]`: the change between them didn't move it, so don't do that
again; try something else, or say why it stands. A rule an encode can cause (blotching) is measured again on the
composition drawn without encoding, so what the footage already holds is a note and only what the encode added is an
error. A take or a download under the project's `assets/` or `media/` is a source, not a delivery: its loudness is
reported as a note, since the mix sets the delivery's level.

## Every rule a project holds

On a project, or a composition's file, with no `--quick`, `raea check` runs every rule that applies to each
composition: raea's floor on its newest render, and every line with a check in your taste, the workspace's
`taste.ts` and the project's own. A rule is such a line: its words, and the check that holds it, one of raea's six
measured forms, one of raea's checks by name with its values, or a `.ts` check of your own by its path.

raea's reading check, `text-read`, runs on the newest render too, with nothing asking for it. Each text's reading
time (0.3 s a word, at least a second) is held to the frames it can be read on (in the frame, at an opacity, size and
contrast that read, and not covered), not the frames it's on, and the finding says which frames were lost and why.
Text that can be read where OCR doesn't read it is an error: it's broken. Render with `--layers` so what covers a
text is counted.

```ts
// taste.ts, in the project
{ text: "Captions clear the button band", check: "checks/caption.ts", values: { band: 0.8 } }
```

A check is a system over the composition's world, the same world `raea world` prints, so it reads a caption, a
footage cut, a cue and a 3D part the same way. It says which frames it ranges over and whether it needs a render, and
calls `found` for each thing that breaks the rule; it never prints.

```ts
// checks/caption.ts
import { check, found, type Layer, type Text } from "raea/check";

export default check({ id: "caption", text: "a caption clears the platform's button band" }, (world, _frame, values) => {
  const band = Number(values.band ?? 0.8);
  for (const t of world.find({ with: ["Text", "Layer"] })) {
    if (world.component<Layer>(t, "Layer")!.medium !== "2d-type") continue;
    const box = world.component<Text>(t, "Text")!.box; // in frame fractions: left, top, right, bottom
    if (box && box[3] > band) found({ about: [t], says: `the caption reaches ${Math.round(box[3] * 100)}% down the frame` });
  }
});
```

What needs a render and has none waits, and the report says what it waits for. A render older than the composition's
last change is still joined, with a note that what it measured may have moved.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea check EDIT.tsx --quick [--shape S] [--platform P] [--taste T] [-o check.json]   (three stills of a composition checked for text: clipped, cramped, safe areas, sizes; about a second)
raea check [PROJECT|EDIT.tsx] [--composition ID] [--only ID] [--taste T] [--render OUT | --no-render] [--json]   (every check that applies to each composition, over its frames: raea's floor on its newest render, the person's, the workspace's and the project's taste.ts (each line's check: one of raea's forms or checks by name, or a project's .ts check); what needs a render and has none waits, and says so)
raea check VIDEO.mp4 [--manifest render.json] [--words W.json] [--perception P.json] [--things T.json] [--shape 16:9|9:16|4:5|1:1] [--platform tiktok|reels|shorts] [--taste taste.ts|DIR] [-o check.json]   (an export against the floor: clipped text, contrast, safe areas, freezes, flashing, cues, loudness, blotching, a placeholder brand kit, and with --things what `raea fingerprint things --json` found: a thing floating or passing through another, the camera lurching mid-move, an empty frame; its findings errors, warns and notes)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--composition` | `ID` | Check one composition of the file or project instead of every one. |
| `--json` | – | Print the findings as JSON (one Finding each, as the project's record keeps them) instead of lines. |
| `--manifest` | `render.json` | The manifest `raea render --manifest` wrote for this export: where each element was on each frame, for the text, contrast and safe-area checks. |
| `--no-render` | – | Join no render: run only the checks that need none, and list the rest as waiting. |
| `--only` | `ID` | Run one rule's check: a check of raea's by name (`text-above`) or a project check's id (its `check({ id })`). |
| `--perception` | `P.json` | Where the face is on each frame (the `perception.json` `raea cut` writes), to find text over a face. |
| `--platform` | `P` | Check the safe areas against this platform's buttons and captions. |
| `--quick` | – | Check a composition's text from three stills instead of checking an export. |
| `--render` | `OUT` | Join this render instead of the composition's newest in the project's record. |
| `--shape` | `S` | The delivery's aspect ratio, for its safe areas. |
| `--taste` | `T` | The person's taste files: the rules in them that carry a `check` are applied too. |
| `--things` | `T.json` | what `raea fingerprint things FILE --json` found in the video (a thing floating or passing through another, the camera lurching mid-move, an empty frame), as findings |
| `--words` | `W.json` | The transcript, to check cues against their words and the voice against the music. |
| `-o` | `check.json` | Write the findings as JSON, for a later review or for `raea next`. |

<!-- generated:end -->

## See also

- [Rendering and checking](../../concepts/rendering.md) lists what the full check measures and how loudness is set.
