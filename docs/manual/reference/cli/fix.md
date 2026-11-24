---
title: "raea fix"
summary: "A problem looked up before you guess: what you see or the error text, \"quoted\" for an exact phrase."
order: 123
sources: ["systems/fixes/src/lib.rs", "sdk/guides/fixes/raea.tsv", "engine/cli/src/cli.rs"]
---

A problem looked up before you guess: what you see or the error text, "quoted" for an exact phrase.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea fix SYMPTOM... [--domain DOMAIN] [--limit N] [--sources] [--notice]   (a problem looked up before you guess: what you see or the error text, "quoted" for an exact phrase; raea's own fixes first, then community skills' and docs')
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--domain` | `DOMAIN` | Only entries of one domain: `bevy`, `blender`, `threejs`, `r3f`, `shaders`, `ffmpeg`, `video`, `audio`, `godot`, `raea` and so on. A domain word in the query itself ranks that domain first without leaving the rest out. |
| `--limit` | `N` | How many library entries to show (6 unless said). raea's own entries, at most three, come above them. |
| `--notice` | – | Print the library's sources and their licences instead of searching. |
| `--sources` | – | Name the sources (repositories, ClawHub skills) each library entry comes from. |

<!-- generated:end -->

`raea fix` is what an agent runs when a frame looks wrong or a command prints an error, before it guesses at a cause.
Say what you see in plain words (`raea fix "shadows cut off"`, `raea fix "glow missing at night"`), or give the error
text inside double quotes, which matches that exact phrase in problems and error text.

It searches two sources, both on this machine, in about a twentieth of a second:

- **raea's own entries** (marked `raea`): problems found and fixed in raea work, each with where and when it was found.
  They live in `sdk/guides/fixes/raea.tsv`, so a new one ships with the next release; an entry shows when it matches
  at least half of the query's words.
- **The library** (numbered): about 46,500 problem → fix entries taken from community skills and docs whose licences
  allow it (MIT, Apache-2.0, BSD, CC0, and every ClawHub skill, which is MIT-0), merged where several say the same
  thing. The number is how many sources state the fix. These weren't checked in raea, and many name another engine's
  API (three.js, Godot, Blender), so translate the idea and confirm the fix on a frame.

The library is a file the release installs (`models/fixes/fixes-library.db` beside raea's other files);
`RAEA_FIXES` names another copy.
