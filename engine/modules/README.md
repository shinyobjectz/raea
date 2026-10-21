# modules

`raea-modules`: loads the agent's TypeScript, TSX and JSX in process, with no bun. It resolves an import inside the
project folder, transpiles the file with deno_ast (swc) using React's automatic JSX runtime, and hands the module to
the JavaScript engine's module loader. Shared libraries (React, `@raea/motion`) come from one prebuilt bundle instead
of being loaded per import. This is what lets compositions run on the iPhone, where there is no bun.

It refuses, with a message the agent can act on, a file outside the project, a missing file, a bare name the bundle
doesn't serve, and a file it can't parse.

Used by [`engine/native/`](../native) (its `modules` feature), [`sdk/react/`](../../sdk/react) and the engine's page scripts.
