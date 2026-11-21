---
title: "raea library"
summary: "Search and manage the person's media libraries in the raea app: folders and drives indexed on this machine, searched by meaning."
order: 30
sources: ["engine/cli/src/library.ts"]
---

Libraries are folders and drives the raea app indexes on this machine, so footage, stills and sounds can be found by
what's in them rather than by file name. `raea library` talks to the app's host, so the app must be running. A folder
an agent adds waits for the person to say yes in the app.

```sh
raea library search "a dog running on a beach" --kind video --limit 5
raea library search "a door creaking open" --sound
raea library new "Stock footage"
raea library add "Stock footage" /Volumes/Footage
raea library status
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea library [new NAME [--cloud] | add LIBRARY PATH | files LIBRARY | tag SHA TAGS | status | scan ...] | raea library search "WORDS" [--library LIBRARY] [--kind KIND] [--limit N] [--sound]   (the person's libraries in the raea app: folders and drives indexed on this machine, searched by meaning; a folder an agent adds waits for the person's yes)
```

Needs the raea app's host running on this machine; then waits for a person's click in the raea app (to add a folder). Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--cloud` | – | With `new`: keep the library's index in raea cloud, searchable from the web and other devices (Pro), rather than on this Mac. |
| `--kind` | `KIND` | With `search`: only `video`, `image` or `audio`. |
| `--library` | `LIBRARY` | With `search`: search this library only. |
| `--limit` | `N` | With `search`: how many results (10 by default). |
| `--sound` | – | With `search`: rank by how things sound, for a described sound effect. |

<!-- generated:end -->
