---
title: "raea skills"
summary: "Install raea's agent skill where Claude Code, Codex, Kimi or Gemini read skills, with its MCP server and hook."
order: 128
sources: ["engine/cli/src/skill.rs"]
---

`raea skills install` copies the skill this raea ships into the folder an agent reads skills from, stamped with this
version: `~/.claude/skills` for Claude Code, `~/.agents/skills` for Codex, Kimi Code's skills folder, and
`~/.gemini/skills` for Gemini. It also adds the Blender MCP server to the agent, and raea's hooks to every agent
([`raea hook`](hook.md)): media opened in the Mac's viewer goes to `raea show`, and the quick check runs after each
saved composition. A skill folder that's a link (a checkout of raea-skills) is left as it is.

The skill raea ships matches the version it ships with. An agent can hold a copy from an older or newer install;
its front matter names the version it came with, and when that differs from `raea --version` the agent reads
`raea skill` instead.

```sh
raea skills install --agent claude
raea skills install --agent codex --no-mcp
raea skills install --agent claude --pin quieter --pin bolder     # /quieter and /bolder as commands of their own
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea skill [NAME] [--reference cli|feed|files|sdk] | raea skills install --agent claude|codex|kimi|gemini|grokbot [--pin MOVE]... [--no-mcp] [--no-hooks]   (the agent skill this raea ships, for its version, which raea --version prints: an agent whose copy names another version reads this; install puts raea's skills where that agent reads skills, adds the blender MCP server unless --no-mcp, makes each --pin MOVE a /MOVE of its own, and adds raea's hooks to that agent unless --no-hooks (raea hook AGENT: media opened in the Mac's viewer goes to raea show, and the quick check after a composition is saved); a skill folder that's a link is left as it is; grokbot, the Grok Bot app, keeps its skills on its own computer, so its copy is staged on this Mac (RAEA_GROKBOT_HOME moves it) with the grokbot:// links that finish it in Grok Bot, and gets no hooks or MCP server)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--agent` | `claude\|codex\|kimi\|gemini\|grokbot` | Which agent to install for. |
| `--no-hooks` | – | Leave Claude Code's settings alone: don't add the quick-check hook. |
| `--no-mcp` | – | Don't add the Blender MCP server to the agent. |
| `--pin` | `MOVE` | Install a note's guide as a skill of its own, so `/MOVE` loads just it (`quieter`, `bolder`, `layout`, `adapt` and the rest). Repeatable. |
| `--reference` | `cli\|feed\|files\|sdk` | Used by `raea skill`; see [`raea skill`](skill.md). |

<!-- generated:end -->
