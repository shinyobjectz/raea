---
title: "raea index"
summary: "Make every picture on a folder or a drive searchable by words, embedded on this Mac's Neural Engine, and keep the index open for the raea app."
order: 31
sources: ["engine/cli/src/cli.rs"]
---

Make every picture on a folder or a drive searchable by words, on this Mac. `add` lists the folder and embeds each
picture's small copy with SigLIP 2 on the Neural Engine, about 170 a second on an M5 Pro, with a counter as it goes;
`search` gives the best matches for words; `status` lists the sources; `remove` takes one out; `serve` keeps the index
open for the raea app, one JSON ask a line on stdin. Nothing leaves the machine.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea index add FOLDER|photos:... [--json] | raea index search WORDS... [--k N] [--source ID|FOLDER] [--json] | raea index status [--json] | raea index remove ID|FOLDER | raea index serve   [--home DIR]   (every picture and video on a folder, a drive or Apple Photos (photos:) made searchable by words on this Mac, fast: add lists it and embeds each picture's small copy with SigLIP 2 on the Neural Engine, a counter as it goes, JSON lines with --json; search gives the best for the words; status lists the sources and what each holds; remove takes one out; serve keeps the index open for the raea app, one JSON ask a line on stdin; the index lives in ~/.raea/library/fast, or --home / RAEA_INDEX_HOME)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--home` | `DIR` | The index's folder (default ~/.raea/library/fast, or RAEA_INDEX_HOME). |
| `--json` | – | Print JSON: progress lines for add, the hits for search, the sources for status. |
| `--k` | `N` | How many hits search gives (10 by default). |
| `--source` | `ID\|FOLDER` | Search only one source, by its id or the folder that was added. |

<!-- generated:end -->
