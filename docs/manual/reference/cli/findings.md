---
title: "raea findings"
summary: "What each render and check found, read back from the project's record: fixed, new and carried since the one before."
order: 46
sources: ["apps/direction/src/findings.rs", "core/facts/src/record.rs"]
---

Every `raea render` and `raea check` of a file in a raea project keeps what it found in the project's record,
`.raea/record/`: one entry a run, with what was rendered or checked, what it wrote and the commit the project was at
(with a `+` when the file had changes not yet committed). `raea findings` reads the record back. For each thing
rendered or checked, it sets the latest run against the one before and says which findings were fixed, which are new
and which are carried, and since which run a carried one has stood. Its last line counts what stands.

```sh
raea findings                          # every composition and video in this project
raea findings piece.tsx                # one composition's renders
raea findings --about crate --open     # only what stands about the crate
raea findings piece.tsx --since 20261110-165445
```

A run is set against the one before it of the same kind: a video against the video before it, a review against a
review, a check against a check. A finding is the same one in a later run when its check and what it is about (or,
about nothing, its words) are the same, whatever frames it is on, so a clip that moves as the piece is cut stays one
finding. Renders that are part of another run, such as a review's tiles or `raea check --quick`'s stills, keep no
entry of their own.

Nothing is fixed by hiding it. An error let through with `--despite` stays standing, and its reason is listed. A part
renamed without moving reads as renamed, with the name it had. A clip the camera turns away from is still said, as a
note that it stands in the scene unseen. A video copied from the export cache says what was found when it was drawn.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea findings [PROJECT|EDIT.tsx] [--about NAME] [--check ID] [--open] [--since RENDER] [--json]   (what each render and raea check found, from the project's record: fixed, new and carried since the one before, or since RENDER; --open: only what stands)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--about` | `NAME` | Only the findings about this part or element (its name, or its key in the code). |
| `--check` | `ID` | Only this check's findings, such as `clips` or `exposure`. |
| `--json` | – | Print the reports as JSON: each thing's latest run, what it is set against, and its new, carried, renamed and fixed findings. |
| `--open` | – | Only what stands: leave out what was fixed. |
| `--since` | `RENDER` | Set the latest run against this earlier one (an entry's name, or its start) instead of the one just before it. |

<!-- generated:end -->

## See also

- [`raea render`](render.md) and [`raea check`](check.md) write the record; [`raea next`](next.md) reads it to pick
  the next step.
