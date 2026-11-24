---
title: "raea notes"
summary: "A person's notes as files the work is held to: each taken word for word, then restated as asks before anything changes."
order: 125
sources: ["apps/notes/src/lib.rs", "apps/notes/src/take.rs", "apps/notes/src/post.rs", "apps/notes/src/check.rs", "apps/notes/src/state.rs", "core/facts/src/note.rs", "sdk/raea/src/note.ts"]
---

A note can hold several things, and a note that is only patched gets one of them answered. `raea notes take` writes
each note the person sent from the raea app (each edit comment, with where it was pinned) or said in a session
(`--said`) to a file in the project, `notes/ROUND/ID.ts`, with their words exactly as written and no asks yet.
Everything taken at once is one round, and a note already taken isn't taken again.

Before anything in the work changes, each note is restated as its asks: one for each separate thing it asks for, the
medium where its words name one (2D, 3D, flat, extruded, real, modelled, voxel), what it is about, and what it doesn't
ask for (`not`), so a change keeps the rest. A note that asks for a redesign is marked so in its file, and its asks say
what is kept.

```sh
raea notes take
raea notes take --said "hold the end card a second longer"
raea notes ls
raea notes post
raea notes check
```

```ts
// notes/1/n1.ts
import { note } from "raea/note";

export default note({
  words: ["I want 2D animated text with occlusion, not 3D text extrusion. Think of how a lyric video would be."],
  from: "app i-show-mw9k-2 cm-4",
  round: 1,
  pin: { t: 12.4, x: 0.52, y: 0.31 },
  asks: [
    { text: "each lyric line is flat type, masked by the singer where they meet", medium: "2D", about: "lyric", not: ["the camera"],
      of: "edit.tsx", check: "../../checks/flat-type.ts", values: { words: "out of the grey", subject: "component:Singer" } },
    { text: "each lyric arrives word by word on its sung time", medium: "2D", about: "lyric", not: ["the font"],
      of: "edit.tsx", proof: { shot: "chorus", sign: true } },
  ],
});
```

Each ask is then held by a check, or by a clip the person signs. A check is a system over the world, as a rule's is
(`raea check`): one of raea's by name, or a `.ts` check by its path from the note's folder, with its `values`. The
skill carries worked ones in `craft/checks` (a part that rides a seat, flat type with the subject in front of it, a
part facing what it serves, nothing of one kind standing on another, a shot held N seconds, no part of a kind);
copy one into the project's `checks/`. `of` names the composition the ask is about, its file from the project
(`#ID` after it where the file registers several): its check runs only there. Where an ask can't be measured, its
proof is a clip: `proof: { shot, sign: true }` (or `frames: [from, to]`).

`raea notes check` runs each ask's check on the work as it stands. A check must fail before the work changes: one that
passes on its first run is refused, since it isn't measuring the ask, and the command exits 1. Once it has failed, the
work may change until it passes. A check changed after it failed is to be shown failing again. The check also cuts
each clip the person is to sign from its composition's newest render (`.raea/notes-clips/ID.mp4`, cut again only when
the render or the frames change). `raea check` runs the notes' checks beside the project's rules, and each finding
says which ask it is for (`[note n1.2]`), so two asks that use one check are told apart.

`raea notes ls` says where each note and each of its asks stands, and what its restatement still owes: a second
reading where several sentences became one ask, a medium its words name that no ask asks for or turns down, and what
a redesign keeps. An ask is to check (no check or clip yet, its check not yet shown failing, or its clip from a
render that's no longer the work: each the agent's move), failing, to sign (its clip waits on the person), passing
or signed; a note reads its asks' worst, and to
restate before any. What each ask has been through is kept in `.raea/notes-checks.json`. It exits 1 when a note's
file can't be read.

`raea notes post` shows each note on its comment in the raea app: its asks once it's restated with nothing owed, where
it stands on the comment's badge (to restate, to check, failing, to sign, passing, signed) and each ask's, and each
clip to sign under its ask. A note shown so already isn't shown again. The person can correct an ask in a word: the
next `raea notes take` writes their correction into the note as a new line of its words, kept, and lists the ask it
corrected in `corrected`, so the note is to restate again until that ask is restated and taken off the list. They
sign a clip in the app, and the next take keeps their sign with the render they watched; a newer render's clip is to
sign again, and a composition changed after its render is to check until it's rendered again. A note is never marked done by the agent: it closes when the
person retires it.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea notes take [PROJECT] [--inbox NAME] [--said WORDS]... | raea notes check [PROJECT] [--composition ID] [--render OUT | --no-render] | raea notes ls [PROJECT] [--json] | raea notes post [PROJECT]   (a person's notes as files the work is held to: take writes each note sent from the raea app or said in a session to notes/ROUND/ID.ts, the words verbatim, to be restated as asks before anything changes, and writes in each correction and sign the person sent back; check runs each ask's check on the work as it stands, which must fail before the work changes (a check that passes first is refused), and cuts each clip the person is to sign from the newest render; ls says where each note and ask stands (to restate, to check, failing, to sign, passing, signed) and what its restatement still owes; post shows each note's asks and where they stand on its comment in the raea app, and each clip to sign)
```

Needs the raea app's host running on this machine (to show post's asks on the person's comments; take and ls run headless). Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--composition` | `ID` | With `check`: check one composition of the project instead of every one. |
| `--inbox` | `NAME` | With `take`: read only this inbox, `~/.raea/inbox/NAME.jsonl` (`RAEA_INBOX_DIR` moves the folder); every inbox when left out. |
| `--json` | – | With `ls`: each note as JSON, with its file, its state and what its restatement still owes. |
| `--no-render` | – | With `check`: join no render, so the checks that need one wait and no clip is cut. |
| `--render` | `OUT` | With `check`: join this render instead of the composition's newest in the project's record. |
| `--said` | `WORDS` | With `take`: a note the person said in this session, taken as written; give it once per note. |

<!-- generated:end -->
