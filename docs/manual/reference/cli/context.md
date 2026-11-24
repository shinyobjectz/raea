---
title: "raea context"
summary: "Print what to read before working on a project: its taste, brief and look, the floor and its modes' guides."
order: 124
sources: ["engine/cli/src/guide.rs", "apps/workspace/src/doctor.rs"]
---

`raea context` gathers, in reading order, everything an agent should read before it touches a project: the person's
taste and the workspace's house rules, the project's brief and look, the floor and the guides its modes call for. It
runs [`raea doctor`](doctor.md)'s checks too, and prints `CONTEXT_STALE` when they find something to fix first.

```sh
raea context ~/Documents/raea/acme/projects/spring-launch
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea context PROJECT   (what to read before working on a project: taste, brief, look, the floor and its modes' guides)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

<!-- generated:end -->

## See also

- [How an agent should use raea](../../start/agents.md) gives the whole order of work.
