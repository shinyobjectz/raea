---
title: "Workspaces and projects"
summary: "Where work lives in raea: a workspace is a git repository of projects, and each project has a brief, its taste and folders for media and renders."
order: 3
sources: ["apps/workspace/src", "core/brief/src", "skills/skills/raea/reference/files.md"]
---

You can render a single `.tsx` file from any folder, and for a quick piece that's all you need. For ongoing work,
raea keeps things in workspaces and projects, so an agent always knows what the work is for, what the person likes,
and where the files go.

## A workspace

A workspace is a git repository, one per studio or client, that holds projects and their house taste. Workspaces live under
your raea root, which is `~/Studio` unless you set another:

```sh
git config --global raea.root ~/Work      # optional: put workspaces somewhere else
raea workspace init acme                  # makes ~/Studio/acme
raea workspace adopt ~/clients/acme       # or makes an existing folder a workspace
raea workspace                            # shows the current one (--json for its projects and paths)
```

## A project

A project is one piece of work inside a workspace. In the raea app it shows as a channel.

```sh
cd ~/Studio/acme
raea project new spring-launch
```

That makes the project's folder:

| Path | Holds |
|---|---|
| `brief.ts` | What the work is: who it's for, the one thing they should believe, its modes, its deliverables |
| `media/` | Footage, music and other source files |
| `out/` | Renders |
| `brand/` | The brand kit: `brand.css`, `brand.json`, logos |

A project can live inline in the workspace or as a repository of its own; both keep its history:

```sh
raea project split spring-launch      # its own repository
raea project join spring-launch       # back inline
```

## The spec files

raea's specs are TypeScript files whose default export is checked against a schema:

| File | What it says |
|---|---|
| `brief.ts` | The project's brief |
| `NAME.script.ts` | A storyboard: scenes of beats, what's on screen, what each scene must land |
| `~/.raea/taste/taste.ts` | The person's taste, which beats any guide's default |
| `taste.ts` | A workspace's or a project's rules, which `raea check` holds every piece to |
| `NAME.parts.ts` | An object's parts, frozen before it's built, checked by raea |

```sh
raea spec brief.ts        # prints the brief as JSON, checked against its schema
```

A brief, written with the `brief` helper from `raea/brief`. Only `title` is required; `believe` (the one thing the
audience should think afterwards) and `mode` are what an agent needs most:

```ts
import { brief } from "raea/brief";

export default brief({
  title: "Spring launch",
  for: "Acme Running",
  audience: "runners who already own our shoes, on their phones",
  believe: "the new sole makes an easy run feel easier",
  mode: ["pitch"],
  deliver: [{ name: "launch-9x16", specs: ["30 s", "9:16", "with sound"] }],
});
```

The full list of each spec's fields is in [Files and specs](../reference/files.md).

## Rules

A standing decision ("every ad is 9:16", "captions clear the button band") is a line in a taste file: the person's
(`~/.raea/taste/taste.ts`), the workspace's or the project's own `taste.ts`. A line raea can measure carries a check:
one of raea's by name, or the project's own `.ts` check, and `raea check` holds every piece to it.

```ts
import { taste } from "raea/brief";

export default taste({
  title: "Acme's rules",
  sections: [{ heading: "Delivery", rules: [
    { text: "Every ad is 9:16." },
    { text: "Captions sit above three quarters of the frame.", check: { form: "text-above", value: 75 } },
    { text: "Loud enough for a phone.", check: "loudness", values: { value: -14 } },
  ] }],
});
```

## Reading a project before working on it

```sh
raea context .
```

prints, in order, everything an agent should read before making anything in the project: the person's taste, the
workspace's taste, the project's taste, the brief, the look, the craft floor and the guides for the brief's modes. See
[How an agent should use raea](../start/agents.md).
