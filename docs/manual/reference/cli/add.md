---
title: "raea add"
summary: "Add, remove and install npm packages for a project, with no Node or npm: raea.lock and node_modules."
order: 28
sources: ["io/npm/src/lib.rs"]
---

A composition can import npm packages. `raea add` resolves them, fetches each tarball once into a store every project
shares (`~/.raea/npm/store`, checked against the registry's hash) and lays out `node_modules` the way pnpm does. The
resolution is kept in `raea.lock` beside `package.json`, so an install with nothing new asks the registry nothing.

```sh
raea add d3-shape@^3 culori
raea add --dev @types/d3-shape
raea remove culori
raea install --offline
```

Install scripts never run, since there is no Node to run them; a package that needs one is named in the report.

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea add PKG[@RANGE]... [--dev] | remove NAME... | install [--offline]  [--in DIR]   (npm packages for the project: raea.lock, node_modules)
```

Headless: it needs nothing running and opens no window. Takes any number of arguments besides its flags.

## Flags

| Flag | Takes | What it does |
| --- | --- | --- |
| `--dev` | – | Add the packages as dev dependencies. |
| `--in` | `DIR` | The project folder holding `package.json` (the current folder by default). |
| `--offline` | – | With `install`: use only what `raea.lock` and the store already have, asking the registry nothing. |

<!-- generated:end -->
