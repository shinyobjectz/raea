# data

`@raea/data`: data-oriented TypeScript for raea.

- `index.ts`: typed components and queries over `@raea/motion`'s ECS (Bevy's World under the engine, TypeScript tables
  under plain bun), and column tables in typed arrays.
- `project.ts`: a project's record kept beside the work: its objects (anything that renders), tasks, questions, notes
  and versions, as an append-only log in `.raea/`. The `raea` project commands in [`cli/`](../cli) call it.
- `brief.ts`, `facts.ts`, `check.ts`: a compiled brief and the facts about its render (words, faces, frames) as
  tables, and a brief's rules checked against the finished video as queries over them.
- `props.ts`: each component's props read from its TypeScript types, so a brief's parts can be checked against them.

Run the tests with `bun test conformance`.

Status: in use by void's agent and [`brief/`](../brief).
