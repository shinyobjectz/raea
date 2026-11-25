---
title: "raea project"
summary: "Make a project in the workspace, list them, or move one between inline and a repository of its own."
order: 24
sources: ["apps/workspace/src/workspace.rs", "apps/workspace/src/storage.rs"]
---

A project is one piece of work in a workspace: its folder, `brief.ts`, `media/` and `out/`. In the raea app it shows
as a channel. `split` turns an inline project into a repository of its own (a submodule) with its history, and
`join` brings it back.

```sh
raea project new spring-launch
raea project new big-shoot --storage submodule
raea project split spring-launch --remote git@github.com:acme/spring-launch.git
raea project join spring-launch
raea project --json
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea project new NAME [--storage inline|submodule] [--client NAME] | raea project tag [NAME] [--client NAME] [--no-client] | raea project split NAME [--remote URL] | raea project join NAME | raea project [--json]   (a project in the workspace: its folder, brief.ts, media/, out/; its client, what raea.app files every card shown from it under (raea cloud's tags: projects don't nest); split and join keep its history)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--client` | `NAME` | The client the project is for: `new` makes it with that client, `tag` sets it. Projects of one client are listed together, and raea cloud tags them the same way. |
| `--json` | – | Print the workspace's projects as JSON: name, path and storage. |
| `--no-client` | – | `tag`: takes the project's client away. |
| `--remote` | `URL` | With `split`: the git remote the new repository pushes to. |
| `--storage` | `inline\|submodule` | With `new`: a folder in the workspace (`inline`) or a repository of its own (`submodule`); the workspace's layout decides by default. |

<!-- generated:end -->
