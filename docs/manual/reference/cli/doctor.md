---
title: "raea doctor"
summary: "Find what doesn't match this version of raea: old brief files, an older workspace format, an agent's stale skill."
order: 53
sources: ["apps/workspace/src/doctor.rs"]
---

`raea doctor` looks for drift between a project and the raea you have: brief files raea no longer reads, a workspace
or project file missing what this version writes or not included from the repository's git config, and an agent's
copy of the raea skill from another version. It reads files and git config only, so it's quick.
[`raea context`](context.md) runs the same checks and says when it finds something.

```sh
raea doctor
raea doctor --fix
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea doctor [--fix] [DIR]   (what doesn't match this raea: old brief files, an older workspace format, an agent's skill from another version)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--fix` | – | Fix what can be fixed safely (set missing keys, add the git include, update the skill) and say what's left. |

<!-- generated:end -->
