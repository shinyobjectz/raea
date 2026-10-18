# native

`raea-native`: raea's one native-call layer. TypeScript running on JavaScriptCore reaches Rust through one function,
`__raea_call(name, json)`. Every name it can call is registered here once, with the TypeScript types of its arguments
and answer, by whichever crate implements it. A host links the crates it wants and calls their `register()`;
`raea-motion types` writes the types `@raea/motion` imports, so a call that doesn't exist or takes the wrong arguments
fails to type-check.

It also holds:

- the JavaScriptCore binding (`src/jsc.rs`) and the web shims bundles expect (`src/web.js`: TextEncoder, URL,
  AbortController, crypto, Blob, ...);
- the shell's native tools: the filesystem, HTTP under a policy, SQLite, tar, HTML to Markdown;
- the flight recorder (`src/trace.rs`): each run's events, memory, stderr, panics and crashes in a session folder.

Nearly every crate depends on it: [`engine/`](../engine), [`react/`](../react), [`media/`](../media),
[`browser/`](../browser), [`void/`](../void).
