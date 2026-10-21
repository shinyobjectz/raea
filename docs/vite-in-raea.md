# Vite apps in raea: Bun built in, and the page engine's DOM gaps (plan, 2026-10-19)

The goal (Shane, 2026-10-19): raea takes a Vite app as it stands, its config, components and CSS, and renders it
frame-exactly, alone (a page) or inside a raea-motion composition beside motion graphics and 3D, with no Bun CLI
and no Node: everything it needs to build and run the app is inside raea. Three workstreams:

- **A. Bun's build crates in raea**: transpile, resolve, bundle and CSS in-process (replacing the 7 places raea
  starts the `bun` CLI), and a Vite app's config honoured.
- **B. Bun's runtime as raea's JS engine (an evaluation, Shane decides)**: bun_jsc + bun_runtime in place of the
  system JavaScriptCore, measured; and Vite itself run on it.
- **C. The page engine's 11 DOM gaps** (jetway-f2's list from rendering Jetflow), each with a fixture and a gate.

Performance and allocation work stays lightspeed's (docs/lightspeed.feature: Z4, R7 and the rest); nothing here
changes [profile.game], sdk/react/js/lite or the bridge without asking it first.

## What we know (checked 2026-10-19)

- Bun 1.4 (August 2026) is Rust: a Cargo workspace of ~127 crates. Build side: bun_transpiler, bun_js_parser,
  bun_js_printer, bun_ast, bun_resolver, bun_bundler, bun_css, bun_sourcemap. Runtime: bun_jsc (bindings to Bun's
  JavaScriptCore build), bun_runtime, bun_event_loop, bun_http and others. Each area keeps its JavaScriptCore glue in
  a separate `*_jsc` crate (bun_css / bun_css_jsc, bun_bundler / bun_bundler_jsc), so the build crates may not need
  JavaScriptCore at all (to be confirmed, A0).
- The crates aren't on crates.io; the workspace calls them internal parts of one binary. raea would take them by git
  at a pinned commit and expect their APIs to move.
- Licensing (Bun's LICENSE.md): Bun's code is MIT. It statically links JavaScriptCore and WebCore, LGPL-2, so a
  program that links them must be available in a form users can relink (object files). That matters for B only, and
  for every exported game if B ships.
- raea already runs Bun 1.4.2 as a CLI (bundle_composition, the session bundler, void's host, workspace and brain).
- raea's page engine is Blitz (HTML/CSS on Servo's Stylo, Taffy, Parley) with the system JavaScriptCore and a small
  hand-written web layer (engine/html/vendor/blitz-v8-script/src/js: bootstrap.js, geometry.js, web.js; engine/html/src/
  gsap_shim.js for the page path). It has no streams, IntersectionObserver, MutationObserver or ResizeObserver beyond
  stubs, and no Web Animations API.
- Measured today: titles' frames run ~50 us a frame on the system JavaScriptCore and ~13 us on bun's (T3); Stylo's
  CSS animation engine is already in raea and seekable (blitz-dom/src/stylo.rs seek_animation).

## A0 and B0: what we found (2026-10-19, Bun 91e7e3b1)

**A0, the build crates.**
- None of bun_transpiler, bun_js_parser, bun_js_printer, bun_ast, bun_resolver, bun_css, bun_bundler or
  bun_sourcemap depends on bun_jsc (their Bun-crate closures, 19-58 crates each, read from the manifests). The
  bundler's entry, `BundleV2::generate_from_cli`, is in bun_bundler; Bun's CLI drives it from
  runtime/cli/build_command.rs (~600 lines of setup to replicate).
- They need Bun's pinned nightly (nightly-2026-09-15; raea builds on stable 1.96), two vendored path deps fetched by
  Bun's scripts (lol-html, rust-argon2, pinned commits), and three generated files from Bun's codegen-only configure
  (`bun scripts/build.ts --mode=codegen`: build_options.rs, json_byte_class.rs, xml_byte_class.rs). With those, all
  four type-check on macOS. (The `cargo` on this machine's PATH is Homebrew's, which ignores rust-toolchain.toml: call
  the nightly's cargo by path.)
- Linked into a scratch binary (`generate_from_cli` referenced): 121 symbols left undefined.
  - ~39 runtime hooks raea never reaches: Bun's link-time dispatch (dev server, the JS event loop), bytecode caching,
    macros, the package manager. Stubs.
  - C/C++ libraries the crates really use: mimalloc (25), Google Highway's SIMD scanning (22), simdutf (12), zlib (1),
    uSockets (2). MIT / Apache / zlib licences: compiled into a glue crate.
  - ~15 small Bun C++ helpers (JS number formatting and parsing, Math.pow for constant folding, collation, a stack
    check, a file URL, CPU features, RSS): reimplemented in Rust (ryu-js for ECMAScript Number-to-string). No WebKit,
    so no LGPL in the build half.
- iOS: not in Bun's targets; bun_core stops at its OperatingSystem enum. A port means 276 macOS-only cfgs across 24
  files: mechanical but a patch to carry. The alternative: the iPhone keeps raea-modules (swc, built for it) and the
  Mac uses Bun's crates.
- Bun's bundler plugins are JS (run by JavaScriptCore through the runtime). Without the runtime, raea's Z3 compile
  (react/js/lite/compile.ts, a JS plugin) becomes a pass before bundling: run in raea's JS engine over the work
  folder, or ported onto Bun's own parser.
- Shape: a `raea-bun` crate built on Bun's nightly as a static library with a C API (transpile, resolve, bundle,
  CSS), the stubs, the helpers and the C libraries in it; raea links it and stays on stable.

**B0, the runtime.**
- Bun's JavaScriptCore is a prebuilt download pinned by commit (oven-sh/WebKit releases, 87ee10da): macOS arm64
  and amd64 only, 156 MB compressed (LTO). No iOS build exists; an iOS one would mean building WebKit for iOS, where
  apps may not JIT anyway, so it would run interpreted, as the system JavaScriptCore does there: no speed gain on iOS.
- Size: Bun 1.4.2 is 61.9 MB, most of it JavaScriptCore and WebCore. Expect tens of MB added to any binary that
  links it (R7: not the exported player, by default).
- Offline builds: the prebuilt tarball can be cached once per pinned commit; building WebKit from source is hours.
- LGPL-2: a binary that links Bun's JavaScriptCore statically must be available in relinkable form (object files),
  per Bun's own LICENSE.md; linking it as a dynamic library instead satisfies that more simply. Shane's call, with
  legal advice, before anything ships with it.

So for B3: B is possible on macOS only (void, raea-motion), and buys speed (T3) and web APIs there; the iPhone and
the exported player stay on the system JavaScriptCore.

## A1 on Rolldown instead (2026-10-19; Shane's call still open)

A0 made Bun's crates look a poor fit: Bun's nightly, 121 link stubs and C libraries, no iOS. Rolldown (Vite's
bundler, MIT, crates.io) has none of that: 1.2.12 builds on raea's rustc 1.96 (oxc 0.152; 1.2.13 and later need
1.97) and for iOS unmodified, and plugins are Rust. A1 was done on it, behind a flag, so the table below still
reads as planned for Bun until Shane decides.
- `bundle/` (raea-bundle): `raea_bundle::bundle(entry)`, Rolldown in process with bun build's settings (IIFE,
  browser, NODE_ENV production, the work folder's tsconfig for JSX and `paths`). Rolldown's own crates are each
  held at 1.2.12 in its manifest (they ask each other for ^1.2.12).
- raea-motion: `RAEA_BUNDLER=rolldown` makes a render's bundle with it (the `modules` feature; the game player
  goes without). Live and baked bundles (`--lite`) are still bun's: their Z3 compile is a JS plugin (A3).
- Measured: pong, titles, animate and solids, five frames each, rendered byte-identical to bun's bundles (20 of 20);
  check.sh titles, animate and solids all 1.000000 (titles edited: 360 of 600 frames stepped); the bundle is half the
  size (titles 468 against 886 KB) and byte-stable across processes (Rolldown's `//#region <path>` comments,
  which named the work folder, are off); bundling ~25 ms for either at load 16.
- A2 (CSS) on the same crate: a Rust Rolldown plugin compiles each `.css` import with lightningcss (MPL-2.0;
  @imports inlined, nesting and color-mix()/oklch() lowered to Safari 13 syntax for Stylo). CSS modules get
  `<file>_<class>_<content hash>` names, so no folder enters them, and `composes` works. The CSS is ordered by
  ESM evaluation (from the import graph) and rides in the bundle's last line (`//# raea-css=<JSON>`), so caches and
  copies carry it. Renders and live play put it on the page after the brand kit (motion.rs page_css).
  sdk/react/bench/css/check.sh: PASS (global CSS, @import, nesting, a module with composes and color-mix). bun can't
  bundle that composition at all ("Import default will always be undefined"), so this is new, not parity.
- Feature unification (lightspeed bisected it to f60e0b7, fixed in d91a34f): Rolldown's crates turn on serde_json's
  preserve_order and arbitrary_precision, and Cargo applies them to the whole binary. In default raea-motion every
  JSON map became an IndexMap and every number its text (pong's apply 87 -> 159 allocations a frame; map order and
  number handling changed for props, the DOM mirror, rdb, bakes, cache keys). raea-bundle is now behind an opt-in
  `rolldown` feature (`cargo build --release --bin raea-motion --features rolldown`). Before it can be default it
  must build apart from raea's graph: its own cdylib behind a C API (bundle(entry, opts) -> js, css, error) or its
  own process, so no dependency's features reach raea's crates.
- Isolation (18d2801): raea-bundle is a cdylib with a C API that raea-motion loads at run time (libloading); it is
  no Cargo dependency of raea, so sdk/react/Cargo.lock is f910193's again and no feature can leak. RAEA_BUNDLE_LIB, the
  binary's folder, or bundle/target/release.
- A3 step 1 (1efbeff): compile.ts's Z3 compile ported to Rust on oxc (engine/bundle/src/z3.rs), site for site on 29 sources.
- A3 step 2 (715fe63): RAEA_BUNDLER=rolldown also makes live and baked bundles, so serve.ts and bun aren't started.
  Baked play is byte-identical to bun's (48 of 48), and pong's live bundle takes 21 ms against 62-66. Rust
  allocations are identical. JS garbage is within run-to-run spread except pong-view, maybe ~1 KB a frame worse
  (unresolved).
- A5 step 1 (b5de295): void's Tailwind compiles in process on raea's JavaScriptCore (raea-native js-exec running
  tailwindcss 4.3.3's compile(); Rust walks the project and reads the stylesheets), byte-identical to bun's
  tw/project.ts on three projects. Left in A5: Tailwind from a Vite app's own CSS entry (`@import "tailwindcss"`
  in a CSS file the bundle imports: the A2 plugin calling the same compile), which needs the bundler decision.
- Left in A3: bundle_entry (react/js/bundle.ts, --node-stub), and the call sites that run TypeScript rather than
  bundle it: brief's props.ts, void's host.rs, workspace.rs (`bun -e`) and brain.rs (brand.ts). Those need a JS runtime with Node's APIs, not a bundler, and that's closer to B. It's
  Shane's call.
- Next: the default flips to Rolldown once Shane agrees; A3 ports compile.ts (Z3) to a Rolldown transform on oxc,
  then serve.ts and the other call sites go.

## A. Bun's build crates in raea

| step | what | done when |
|---|---|---|
| A0 | Feasibility, a day: build bun_transpiler, bun_resolver, bun_css, bun_bundler from git in a scratch crate; list what each pulls in (bun_jsc, boringssl, mimalloc?); build them for aarch64-apple-ios; read their entry points | a note: which crates build without JavaScriptCore, on macOS and iOS, and the API to call |
| A1 | Transpile and resolve in-process: TS/TSX/JSX through bun_transpiler (React's automatic runtime, as today), imports through bun_resolver (tsconfig `paths`, package `exports`, extra module folders) | raea-motion bundles pong, titles and the timelines with no `bun` process, bundles byte-identical or render-identical to the CLI's (replay, check.sh 1.000000) |
| A2 | CSS: `import "./x.css"`, CSS modules, nesting, through bun_css; the CSS a bundle imports reaches the page | a fixture with CSS imports and a module renders as Chrome does |
| A3 | Replace the CLI everywhere: bundle_composition and the lite bundle (react/js/lite/build.ts and serve.ts become calls into the crates; the Z3 JSX compile stays as a transform pass), void's host, workspace and brain | `rg 'Command::new\("bun"\)'` finds nothing; void's edit-to-swap no slower (33-61 ms); every gate passes |
| A4 | A Vite app's config: aliases, `define` and `import.meta.env` read from vite.config (evaluated in raea's JS engine, or a declared subset) and from tsconfig; packages from the app's own or a shared node_modules; Vite's asset queries (`?url`, `?raw`, `svg?react`), `import.meta.glob` | Jetflow's components (the `@/` alias, packages/npm) import into a raea-motion composition and render |
| A5 | Tailwind v4 from the app's own entry (`@import "tailwindcss"`, its @theme and @source): Tailwind's compiler (JS) run inside raea's JS engine, its class scanner (tailwindcss-oxide, Rust) as a crate | Jetflow's styles.css compiles in raea; its screens match a Vite build's in Chrome |

Not covered by A: a Vite app's own JS plugins beyond React and Tailwind. They run only if B's step B4 works (Vite on
an embedded runtime); otherwise such an app builds with its own Vite outside raea and raea renders the output.

## B. Bun's runtime as raea's JS engine (an evaluation; Shane decides)

| step | what | done when |
|---|---|---|
| B0 | Feasibility: bun_jsc and bun_runtime built into a scratch raea binary on macOS; on iOS (JIT isn't allowed there for apps: does Bun's JavaScriptCore build run interpreted?); WebKit fork's build offline (prebuilt libraries?); the LGPL plan (object-file distribution for raea and every exported game) | a note with yes/no on each and the binary size added |
| B1 | Behind a flag: raea's JS host (sdk/react/src/lib.rs js_thread_jsc, the page engine's script layer) on bun_jsc instead of the system framework | pong, titles and a page render identically (replay, check.sh, parity corpus unchanged) |
| B2 | Measure as lightspeed's rows do: T3 (check.sh titles, incremental and full bake; the logged lever is ~4x a JS frame, ~50 against ~13 us), R1/R2 (jsheap.py on pong's three fixtures: another JavaScriptCore changes its garbage too), F5, R7 (player size), and which of the missing web APIs bun_runtime brings (streams, AbortController, text streams, fetch, Blob, URL) | a table for Shane: speed, size, coverage, licence |
| B3 | Shane's decision: ship B (raea's engine is Bun's JavaScriptCore everywhere it's allowed), keep it for authoring only, or drop it. Default for the exported game player: it keeps the system JavaScriptCore (R7: every byte in the player counts against 65 MB, and a vendored WebKit is many times what R7's splits save), unless the bytes are counted and accepted | decided |
| B4 | Vite itself on the embedded runtime: `vite build` (Rolldown, plugins and all) run inside raea, no Node | a Vite app with a non-trivial plugin builds in raea as `vite build` does under Node |

## C. The page engine's 11 DOM gaps

Each gets a fixture first: a small standalone page from jetway-f2 (asked 2026-10-19), no Jetflow code, that shows the
gap in raea and works in Chrome. Layout and paint items are gated against Chrome (Q2's parity harness, its corpus
unchanged at 93 exact / 75 ok / 2 fail or better); API items by a behaviour test in the page engine. Source to take
from in brackets.

| # | gap | fix | where |
|---|---|---|---|
| C0 | (cross-cutting) time | Every clock a page reads advances with the frame being rendered, in renders and in play: Date.now, performance.now, timers, requestAnimationFrame, CSS animations and transitions, Web Animations. The base #8 and #9 sit on | engine timers (blitz-v8-script runtime.rs, timers.rs), the page path's seek (engine/html/src, __timelines) |
| 1 | streams: ReadableStream, WritableStream, TransformStream, TextEncoderStream, TextDecoderStream | from bun_runtime if B ships; otherwise WebKit's JS-builtin streams (made for JavaScriptCore) or the WHATWG reference implementation in the web layer | blitz-v8-script/src/js |
| 2 | no HashChangeEvent | an Event subclass, and hashchange fired on history changes | web.js |
| 3 | no PublicKeyCredential | a stub that reports the feature unsupported (isUserVerifyingPlatformAuthenticatorAvailable resolves false) | web.js |
| 4 | IntersectionObserver missing or never fires | native: intersections computed from Blitz's layout after each frame's layout, callbacks delivered before paint, deterministic per frame (after #6, so transformed boxes are right) [W3C polyfill as a stopgap] | blitz-dom + web layer |
| 5 | AbortController.abort throws inside dispatch | the dispatch bug at web.js:333 (`_abort`); a test with an unmount cleanup | bootstrap.js / web.js |
| 6 | getBoundingClientRect ignores ancestors' transforms | the box mapped through every ancestor's transform (and perspective) to the viewport [Servo's layout as reference] | blitz-dom geometry |
| 7 | position: fixed ignores a transformed ancestor's containing block | a transformed (or filtered, or will-change: transform) ancestor is the containing block for fixed descendants, as CSS Transforms says [Servo's layout] | blitz-dom / Taffy layout |
| 8 | CSS animations held at their first keyframe on the page path | the page path's clock (a GSAP timeline's seek) reaches Stylo's animation seek, which compositions already use (seek_animation) | engine/html/src page path, blitz-dom/src/stylo.rs |
| 9 | motion/react's frameloop never runs between seeks | requestAnimationFrame callbacks run once per rendered frame at that frame's time (C0); Web Animations (element.animate, which motion uses) on Stylo's animation engine | engine timers, web layer |
| 10 | inputs paint the value attribute, not the property; native input events don't reach React's root listener | the value property drives what's painted; native input, change and focus events dispatched through the DOM so React's root listener sees them | blitz-dom forms, event dispatch |
| 11 | CoreText asks for .LucidaGrandeUI and falls back to Times | system-ui and Apple's private UI names mapped to SF Pro and its fallbacks [Servo's fonts: macOS fallback lists] | fontique / Parley font mapping |

C status (2026-10-19): C0 (ba51023, opt-in frames clock), #2/#3 (81e4a25), #5 (2e9ca65), #8 (cf8ee9c), #11 (4b63877),
#6 (f82ebe8: getBoundingClientRect through transforms, 7 cases = Chrome 152). #7: the containing block was already
right (a transformed, filtered or will-change ancestor contains fixed boxes; engine/html/tests/page_fixed.rs, 8 cases =
Chrome 152). Found while checking it, not fixed: paint order. A box hoisted past its layout parent (a fixed box
contained by the viewport, an absolute box by a far ancestor) paints after the whole subtree of the layout child it
sits under, where Chrome paints positioned boxes and z-index 0 stacking contexts together in tree order (CSS 2.1
Appendix E step 8): a fixed overlay early in <body> covers later transformed boxes in raea, not in Chrome. Fixing
it means a per-stacking-context list for step 8 with each box's ancestor clips carried along, and the parity corpus
has to gate it.
#4 (d39aca1): IntersectionObserver from the page's layout, checked every frame before the paint (script geometry
now resolves at the frame's time, not 0). #9 (21a03d4): Web Animations on the composition's timeline, and motion
12's animate() runs frame by frame (frames clock). On the way: SVG elements got their own prototype (motion took
every div for SVG), requestAnimationFrame passes the frame's time (it passed a constant 16), NodeList and
HTMLCollection exist. #10 (fbce6ce): a control's value apart from its value attribute (dirty value flag,
defaultValue), native events in the DOM's three phases and `instanceof Event`. #1 (a799c6e): the Streams API
(web-streams-polyfill 4.3.0, loaded lazily), TextEncoderStream/TextDecoderStream, Response.body, Response from a
stream, Blob.stream, ProgressEvent. All 11 items have tests; parity 95 / 75 / 0 throughout. Still to show:
Jetflow's tutorial rendering with jetway's stubs removed (needs jetway's app), and the paint-order gap above.

Done for C: each fixture renders as Chrome does (or behaves as specified), and Jetflow's tutorial renders with every
one of jetway's stubs removed.

## Order

1. A0 and B0 together (a few days): what builds, where, at what size, under what licence. Shane decides B's scope then.
2. A1 to A3: Bun out of raea's process list. Meanwhile C0, #5, #2, #3, #8, #11 (small, independent).
3. A4, A5: Jetflow's components into a composition. Meanwhile #6, #7, then #4, #9, #10, #1.
4. B1 to B4, if Shane says go after B0.

The finish line: Jetflow's tutorial video built and rendered by raea alone (no Bun CLI, no Node), its components mixed
into a raea-motion timeline, and none of jetway's stubs.

## Owners and files

- A and B: an agent session. Touches sdk/react/src/motion.rs (bundling), react/js/lite/build.ts and serve.ts, void's host,
  workspace and brain, a new crate for the Bun build pieces.
- C: an agent session (Shane asked for it directly, 2026-10-19; no other agent holds it). Touches engine/html/vendor/blitz-v8-script, engine/html/vendor/blitz-dom, engine/html/src's page path, font mapping.
  engine/html/docs/parity-plan.feature (@owner:unicorn) gets the layout items' parity cases.
- lightspeed: performance; consulted before any change to sdk/react/js/lite, the bridge or [profile.game]. Z4 builds
  on JavaScriptCore's C API (JSObjectMakeArrayBufferWithBytesNoCopy for its op ring): if B changes the API the host
  calls, lightspeed hears first. Its files in flight (2026-10-19): media (soft audio / h264 split), rdb's h264 gates,
  the Cargo feature lists of react, mixer, void and sdk/react/ios, sdk/react/src/zeroheap.rs, lib.rs and play.rs.
