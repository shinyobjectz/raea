# engine

`raea`, the core crate: HTML and CSS rendered without a browser, and the pieces other crates draw with.

- The renderer: Blitz's DOM, Stylo for style, Taffy for layout, Parley for text, and Vello (or Skia) for paint. Page
  scripts run on JavaScriptCore through `vendor/blitz-v8-script`, and GSAP timelines are evaluated natively, so every
  frame is a function of time.
- The `raea` CLI (`src/main.rs`): `probe`, `render`, `snapshot` and `lint` for HTML compositions, plus `measure` and
  `previz`.
- The quads compositor (`src/quads`), the stage and the window code void draws its chrome and stage with.
- A Rust session API (`src/api.rs`) and a C ABI over it (`src/ffi.rs`, header `include/raea.h`) for apps that link it
  as a library; `scripts/` builds the dylib and the iOS xcframework.
- `vendor/`: patched copies of Blitz, Stylo, Vello, Parley, fontique and anyrender. Each has a `NOTE.md` or header
  naming its changes. `fonts/` holds bundled OFL fonts.

Almost every other crate depends on this one by path: [`sdk/react/`](../../sdk/react), [`engine/rdb/`](../rdb), [`apps/void/`](../../apps/void),
[`engine/browser/`](../browser).

Status: in use; the HTML composition path is the oldest part of raea.
