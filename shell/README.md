# shell

`@raea/shell`: the agent's shell. [just-bash](https://github.com/vercel-labs/just-bash) (Apache-2.0, pinned, used
unmodified) interprets the bash: pipes, redirections, functions, loops and about 80 text tools, in TypeScript on
JavaScriptCore. Under it, the filesystem is the project folder and nothing else, and raea's own tools (`rdb`,
`raea-motion`, `sqlite3`, `js-exec`, media verbs) are commands that call the engine natively, in process.

Every command line is one row in `.raea/actions.jsonl`, with the files it read and wrote and their contents kept by
hash, so any command can be undone and redone.

- `src/`: the shell (`index.ts`), its filesystem (`fs.ts`), raea's commands (`commands.ts`) and the JSON-lines server
  void talks to (`serve.ts`, run by `raea-motion shell-serve`).
- [`shell.feature`](shell.feature): the behaviour spec.
- `conformance/` and `probe/`: tests whose answers must match on the Mac and the iPhone; `tools/upstream.sh` runs
  just-bash's own suite.

Status: in use by void's agent.
