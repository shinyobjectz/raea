---
title: "raea workspace"
summary: "Make, adopt or show a workspace: the git repository that holds a studio's projects and house taste."
order: 23
sources: ["apps/workspace/src/workspace.rs"]
---

A workspace is a git repository, one per studio or client, under your raea root (`~/Documents/raea` unless
`git config raea.root` says otherwise). It holds projects and the house taste in `taste.ts`. Every
change raea makes in it is a commit.

```sh
raea workspace init acme                   # a new workspace, ~/Documents/raea/acme
raea workspace adopt ~/clients/acme        # an existing folder made a workspace in place
raea workspace --json                      # the current workspace, its projects and their paths
```

`adopt` moves and deletes nothing: it makes the folder a git repository if it isn't one, and registers each folder
holding a `brief.ts` as a project. `raea workspace init` inside a cloned workspace sets it up again on that machine.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea workspace init [NAME] [--layout LAYOUT] | raea workspace adopt DIR [--name NAME] | raea workspace [--json]   (a workspace: a git repo of projects under raea.root, ~/Documents/raea unless git config says)
```

Headless: it needs nothing running and opens no window. Takes up to 2 arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--json` | – | Print the workspace as JSON: its name, layout, projects and their paths. |
| `--layout` | `LAYOUT` | How its projects are stored: `monorepo` (folders in the workspace's own history, the default) or `submodules` (each project a repository of its own). |
| `--name` | `NAME` | With `adopt`: the workspace's name, in place of the folder's. |

<!-- generated:end -->

## See also

- [Workspaces and projects](../../concepts/workspaces.md) explains what lives where.
