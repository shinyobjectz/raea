---
title: "raea hook"
summary: "Every agent's tool hooks: media opened in the Mac's viewer goes to raea show, and the quick check after a composition is saved."
order: 54
sources: ["apps/check/src/hook.rs", "apps/tools/src/agent_hooks.rs"]
---

`raea hook` is run by an agent's harness, not by hand: Claude Code, Codex, Gemini CLI and Kimi Code each call it from
their own hook settings, which `raea skills install --agent A` adds (and leaves alone when an entry is already there).
Inside the raea app the same hooks hold, since the app runs each agent with its own shell and config.

Before a shell command, it looks for a still, a video or a sound being opened in the Mac's viewer or a browser
(`open`, `open -a Preview`, `xdg-open`, `qlmanage -p`). While the raea app is installed it stops that command once and
tells the agent to use [`raea show`](show.md) instead, so the work comes up in the app as something the person can
answer. The same command run again in that session within ten minutes goes through, for when the person asked for the
viewer. `open -a raea`, Finder (`open -R`), links and PDFs are never stopped.

After a file is saved, it runs `raea check --quick` on a composition and hands the findings back to the agent, so
clipped or cramped text is caught on the edit that caused it. A stop is exit 2 with the reason on stderr, which every
harness hands its agent; findings go back in the shape each harness reads.

```sh
raea skills install --agent claude     # the skill and its hooks, for Claude Code
raea skills install --agent kimi       # the same for Kimi Code (codex and gemini too)
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea hook [claude|codex|gemini|kimi]   (every agent's tool hooks, which raea skills install adds: before a shell command, media opened in the Mac's viewer while the raea app is installed is stopped once for raea show (the same command again goes through); after a save, the quick check on a composition)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

<!-- generated:end -->
