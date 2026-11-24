---
title: "raea ask"
summary: "Ask about a piece's picture and sound: clef-omni answers in words and a probability, paid from your raea plan, and every answer is kept so a question can be calibrated."
order: 131
sources: ["apps/ask/src/verbs.rs", "apps/ask/src/prepare.rs", "apps/ask/src/runner.rs", "apps/ask/src/labels.rs", "apps/ask/src/calibrate.rs", "io/clef/src/route.rs", "core/ask/src/library.rs"]
---

`raea ask` puts questions about a video, a sound or a picture to clef-omni, the decision model raea's checks use, and
prints each answer in words with its probability, the window it looked at and what it cost. Ask in your own words, or
use the standard questions: the craft rubric's (`rubric`), picture and sound together (`av`), the defects the
fingerprint can't judge yet (`defects`), the platform checks (`platform`), picking between versions (`decide`) and the
review loop's gate questions about a [`raea make`](make.md) job's sheets (`make`). `raea ask list` lists them all.

Before it spends anything it reads what it can about the piece: what the file holds, the project's brief (the nearest
`brief.ts` above the file, or `--brief`) and whether anyone speaks (`--words`, `--voice`). A question that doesn't
apply isn't asked, and its line says why: a sound question of a silent file, or the rubric's `sound` question of a piece
where no one speaks, which asks `sound-silent` instead. Every question about the same clip goes in one call, and
many questions cost about what one does.

It pays from your raea plan's monthly pool when the raea app is signed in on Pro or Studio, or with an agent key
(`RAEA_AGENT_KEY` with `RAEA_API`) where the app isn't running, as in a pipeline or CI. Otherwise it uses your own
Cloudflare keys ([settings](../../settings.md)). When the pool is spent, the questions not yet asked wait for next
month, and their lines say so. The same question about the same media comes from a cache and costs nothing.

```sh
raea ask out/piece.mp4 "Does the music fight the voice?"
raea ask out/piece.mp4 --set av --brief brief.ts
raea ask out/piece.mp4 "Which ending lands harder?" --with out/v2.mp4 --std decide.best
raea ask out/piece.mp4 "Is the logo readable?" --at 12.5 --flag no
raea ask out/piece.mp4 --set defects --dry                 # what would be sent and its cost; nothing is spent
raea ask --batch asks.jsonl --json --gate                  # a pipeline: one JSON line an answer, exit 1 on a finding
```

A batch is a JSON list, or JSON lines, of `{file, questions?, set?, std?, window?, with?, brief?, words?, voice?,
state?}`, with `-` reading stdin. Its clips are each cut once, and its calls go four at a time (`--jobs`). Scripts can
ask through [`raea/ask`](../sdk/ask.md) instead.

Answers are advice until a question is calibrated against your own eye, and the answers say which they are. With
`--gate`, a calibrated question that flags exits 1; with `--gate any`, any question that flags does. Every call and
answer is kept in `~/.raea/ask` (`RAEA_ASK_DIR`), with the clips it cut, so that a question can be labelled and
calibrated later.

A question learns from three kinds of label. Your verdict on an answer comes first: `raea ask verdict ANSWER WORD`,
with the answer's id from `--json` and your own answer to the question (yes or no, an option, a level). Then your notes:
a note of yours on the render in the project's feed, inside the window it looked at, that holds one of the question's
label words says the problem was there, and a post you approved with no such note says it wasn't. An agent's own look
(`--eye agent`) is kept beside them and never counted.

`raea ask calibrate` reads your notes again and holds each labelled question to its labels. One with 16 labels of each
kind, from 3 projects, and an AUC of 0.8 or more is calibrated: it flags where 10% of your clean work would, its answers
are findings, and your calibration replaces the one raea ships. One under 0.6 after 32 labels doesn't separate, and
lists the labels it missed so its words can be fixed. `raea ask try "new words" --like ID` asks new words of that
question's labelled clips, kept from before, and prints both AUCs on the same labels.

```sh
raea ask verdict a-3f09c2e81b44 yes                        # yes, the music does fight the voice there
raea ask calibrate                                         # every labelled question, where it stands
raea ask try "Is the voice hard to hear over the music?" --like own-does-the-music-fight-the-voice
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea ask FILE [QUESTION...] [--choose A,B,C] [--grade] [--rate N] [--flag ANSWERS] [--set NAME] [--std ID] [--from S] [--to S] [--at S] [--with FILE2] [--brief brief.ts|none] [--words words.json] [--voice yes|no] [--state TEXT] [--dry] [--json] [--gate [any]] [--jobs N] | ask --batch FILE [--dry] [--json] [--gate [any]] [--jobs N] | ask list [SET] [--json] | ask verdict ANSWER WORD [--eye NAME] | ask calibrate [ID] [--json] | ask try WORDS --like ID [--dry] [--json] [--jobs N]   (questions about a piece's picture and sound, answered by clef-omni and paid from your raea plan's pool when the app is signed in, else RAEA_AGENT_KEY's, else your own Cloudflare keys: your own words, or the standard sets (rubric, av, defects, platform, decide, make); every question about one clip goes in one call, --batch (a JSON list or JSON lines, - for stdin) asks many clips in parallel; --dry says what would be sent and spends nothing; answers are advice until a question is calibrated: verdict labels an answer, calibrate holds each question to your verdicts, notes and approvals, try asks new words of its labelled clips)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--at` | `S` | The three seconds around this moment: every question looks at that window. |
| `--batch` | `FILE` | A file of entries to ask, a JSON list or JSON lines (`-` reads stdin); each is a file with its questions. |
| `--brief` | `brief.ts\|none` | The project's brief, so clef knows who it's for and what it should do (else the nearest brief.ts above the file; `none` for no brief, when `--state` says what the piece is). |
| `--choose` | `A,B,C` | Your questions pick one of these options, rather than answering yes or no. |
| `--dry` | – | Cut the media and print each call that would be made (its state, questions, media sizes) with an estimated cost, and send nothing. |
| `--eye` | `NAME` | With `verdict`: the verdict is another eye's (an agent's own look), kept and never counted toward calibration. |
| `--flag` | `ANSWERS` | The answers that mean a problem: `no` for a question where no is bad ("Is the logo readable?"), or a choice's options. |
| `--from` | `S` | Where the window starts, in seconds. |
| `--gate` | – | Exit 1 when a calibrated question flags, or questions wait for the pool; `--gate any` exits 1 when any question flags. |
| `--grade` | – | Your questions answer on a graded scale (poor, weak, acceptable, strong, excellent). |
| `--jobs` | `N` | How many calls go at once (4). |
| `--json` | – | One JSON object a line: each answer, each call `--dry` would make, then the summary. |
| `--like` | `ID` | With `try`: the question whose labelled clips the new words are asked of. |
| `--rate` | `N` | Your questions answer with a score from 1 to N. |
| `--set` | `NAME` | A standard set to ask: rubric, av, defects, platform or decide (comma-separated for several). |
| `--state` | `TEXT` | More for clef to read before the media: what you measured, the platform it's for. |
| `--std` | `ID` | Standard questions by id (`raea ask list`), comma-separated. |
| `--to` | `S` | Where the window ends, in seconds. |
| `--voice` | `yes\|no` | Whether anyone speaks in it, when no words file says: it decides which of the voice questions' variants is asked. |
| `--with` | `FILE2` | Another file beside this one, to pick between or compare (repeat it for up to four in all). |
| `--words` | `words.json` | A words file (a JSON list of the words heard): clef reads what is said, and none heard means no one speaks. |

<!-- generated:end -->
