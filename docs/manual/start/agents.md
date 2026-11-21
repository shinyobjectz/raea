---
title: "How an agent should use raea"
summary: "The order of work for a coding agent using raea: read the project, write, check, look, render, check again, hand over; and which steps run headless."
order: 4
sources: ["skills/skills/raea/SKILL.md", "engine/cli/src/guide.rs", "apps/workspace/src"]
---

This page is for coding agents (Claude Code, Codex, Kimi, Gemini and others) and for the people who set them up.
After reading it, an agent knows the loop raea expects, which commands to run at each step, and which steps need a
person.

## Where each step runs

Every step below is marked with where it can run:

| Mark | Means |
|---|---|
| HEADLESS | The `raea` command alone, in any shell, with no window and no app |
| HOST | Needs the raea app running on this machine |
| PAGE | Needs the person's feed open in the raea app |
| PERSON | Only the person can do it, such as approving a storyboard or watching a cut |

An agent with only a terminal can do every HEADLESS step, which covers making, checking and delivering. Each PAGE
step below says what to do headless instead.

## 1. Install the skill (once) — HEADLESS

```sh
curl -fsSL https://raea.io/install | sh            # installs raea and the skill for Claude Code
raea skills install --agent codex                  # or kimi, gemini: the skill where that agent reads skills
```

If the skill your agent loaded names a different version than `raea --version`, read `raea skill` instead. It prints
the skill as the installed raea ships it.

## 2. Read before you make — HEADLESS

In a project folder, print everything to read first, in order:

```sh
raea context .
```

That prints the person's taste (`~/.raea/taste/taste.ts`), the workspace's taste, the project's own taste, its brief,
its look, the craft floor, and the guide for each of the brief's modes. Read all of it. A line in the person's taste
beats any guide's default.

With no request in hand, `raea next` lists the two or three useful next steps for the project. Offer them to the
person; don't run one unasked.

Right before any edit to a composition, even a one-line change, read the craft floor:

```sh
raea guide craft
```

`raea guide` alone lists every guide: the modes (pitch, explain, story, play, tool), motion, music, sound, layout,
words, games, rendering, and `bevy` with its topics for 3D.

## 3. Write the composition — HEADLESS

A composition is one `.tsx` file. The rules that keep a render right:

- **Frame-pure.** Everything that moves reads `useCurrentFrame()`. `useState`, `useEffect` and timers don't animate
  a render.
- **Clamp every `interpolate`** unless you mean the value to run past its ends.
- **Cue on words, not guessed frames.** `word("phrase")` is the frame where the phrase is said, so cues follow the
  cut. See [Captions and cues on words](../guides/words.md).
- **Games are a pure step.** `useGameState(initial, step)`, with keys from `useInput()`. See
  [Your first game](first-game.md).

[Compositions and frames](../concepts/compositions.md) explains why, with examples.

## 4. Check it in a second — HEADLESS

After every save:

```sh
raea check piece.tsx --quick
```

It renders three stills and measures their text (clipped, cramped, outside the safe areas, too small) in about a
second. With Claude Code, the installer's hook runs it for you after each save and hands you what it finds.

## 5. Look at frames — HEADLESS

Render the moments that matter as PNGs and look at all of them in one batch: each cut, each part's entrance and
settled state, the first half second, the end card.

```sh
raea render piece.tsx --at 0,45,89 -o frames/
```

Fix everything the batch shows, then render the batch again. Don't render one still per change.

## 6. Render and check the export — HEADLESS

```sh
raea render piece.tsx -o piece.mp4 --manifest render.json
raea check piece.mp4 --manifest render.json -o piece.check.json
```

`raea check` measures what a still can't: text under a platform's buttons, captions on too briefly to read, caption
contrast against the footage behind them, frozen stretches, flashing, cues that miss their words, loudness, and
blotching in dark areas. Each finding is ranked P0 to P3 with its fix. It exits 1 while a P0 or P1 stands. Don't hand
over a render while `raea check` still names a fault.

## 7. Show it and take notes — PAGE, PERSON

In the raea app, post the render to the project's feed. The person watches or plays it there and leaves notes.

Headless instead: leave the file in the project's `out/` folder, give the person its path, and take their notes as
text.

## 8. The next version — HEADLESS

- Render each version to a new file beside the old one (`piece-v2.mp4`), never over it, so earlier versions stay
  watchable.
- Keep the person's hand edits. Edits made in the raea app come back as props on the composition (`edit` segments
  and `<Nudge>` offsets). Change the composition under them.

## 9. Deliver — HEADLESS

- Final exports of motion graphics and type: `--quality master`.
- Name files with lowercase letters, digits and hyphens only: `spring-launch-9x16.mp4`.
- Deliver to the taste when it sets sizes, platforms or loudness (`raea context .` prints it).

## What an agent should never do

- Never invent a flag. `raea help` lists every verb and flag, and the [CLI reference](../reference/cli.md) is
  generated from it.
- Never render over a previous version.
- Never call a piece done without looking at its frames and running `raea check` on the export.
- Never add a folder to the person's libraries or approve a storyboard on their behalf. Those wait for the person.
