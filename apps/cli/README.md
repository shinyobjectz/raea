# cli

`@raea/cli`: `raea`, the agent's one command for working on a project as a React + TypeScript folder. It runs inside
[`@raea/shell`](../../sdk/shell), so it behaves the same on the Mac and the iPhone.

Commands cover the project (`init`, `add`, `cut`, `brand`, `status`, `objects`, `tasks`, `notes`), its versions
(`save`, `versions`, `diff`, `checkout`, `restore`), rendering and checking (`render`, `still`, `frame`, `check`,
`sheet`, `script`), and the agent's guides (`craft`, `skills`). What needs perception or the renderer is passed to the
host (void on the Mac) to run.

- `template/`: the files `raea init` writes into a new project.
- `registry/`: ready-made parts (captions, callouts, lower thirds, end cards, ...) that `raea add` copies in.
- `tools/pack.ts` builds `src/registry.gen.ts` from them; `conformance/` tests the command surface.

Status: in use by void's agent.
