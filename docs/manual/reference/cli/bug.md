---
title: "raea bug"
summary: "Report a bug in raea itself to raea's server: a line saying what's wrong and what happened, with raea's version and the OS added, no account needed."
order: 130
sources: ["engine/cli/src/bug.ts", "engine/cli/src/cli.rs"]
---

Report a bug in raea itself to raea's server: a line saying what's wrong and what happened, with raea's version and the OS added, no account needed.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea bug TITLE... --text TEXT [--command CMD] [--error TEXT] [--agent NAME] [--dry-run]   (a bug in raea itself, not in your own composition, sent to raea's server with raea's version and the OS, no account needed: words only, paths, emails and keys taken out before it goes; five a day from a machine, the same bug again counted on the first report; --dry-run prints what would go and sends nothing)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--agent` | `NAME` | Who is reporting: the agent or harness's name (claude-code, codex, kimi). Unless said, `RAEA_AGENT`, else the harness raea finds it running under. |
| `--command` | `CMD` | The raea command that went wrong, as you ran it (paths in it are taken out). |
| `--dry-run` | – | Print exactly what would be sent, raea's version and the OS included, and send nothing. |
| `--error` | `TEXT` | What the command printed when it went wrong, up to 4,000 characters (paths, emails and keys taken out). |
| `--text` | `TEXT` | What you did, what you expected and what happened instead, up to 4,000 characters. Needed. |

<!-- generated:end -->

`raea bug` is how an agent tells raea about a bug it thinks it found in raea itself: a command that fails or hangs, a
result that's wrong, or help that says one thing while raea does another. It's for raea's bugs, not the composition
you're writing. It works in any shell, headless or in the app, and needs no account.

The report is words only. raea adds its version and the OS, and takes out paths, emails and anything shaped like a
key or token before it leaves the machine; raea's server takes them out again. A machine sends at most five a day,
and the same bug sent again (by the same line, numbers and paths aside, on the same version) is counted on the first
report instead of being kept twice. `--dry-run` prints exactly what would be sent and sends nothing.

```sh
raea bug "raea render stops at frame 12 with no error" \
  --text "Rendered compositions/teaser.tsx to mp4; it printed frame 12 of 240 and exited 0 with no file." \
  --command "raea render compositions/teaser.tsx -o out/teaser.mp4" --error "frame 12/240"
```
