@spec
Feature: One way to run TypeScript in Rust
  raea runs TypeScript on its own V8 (rusty_v8 152, jitless outside local benches), the same on the Mac and the iPhone,
  and everything TypeScript reaches in the engine goes through one native-call layer. This is how @raea/motion, the
  agent's shell and browser, the engine's page scripts and the tests all run.

  Rule: One native-call layer (native/, raea-native)
    Scenario: The registry
      Then every native call is registered once by name with the TypeScript types of its arguments and answer, by the
        crate that implements it: raea-native's own (host.*, web.*, url.*, zlib.*, fs.*, net.*, sqlite.*, tar.*,
        html.*, js.run), raea-react's (solid.*, ecs.*, vdec/venc/adec/aenc.*, frame.*, media.*, cli.*), the sandbox's
        (wsx.*) and the browser's (browser.run)
      And `raea-motion types -o motion/src/native-calls.ts` writes the table @raea/motion's `native(name, args)` is
        typed by, so a call that doesn't exist or takes the wrong arguments fails to type-check
    Scenario: The V8 binding
      Then raea-native's v8 feature starts V8 once per process (ICU data for Intl; the Mac's heap at 4 GB) and puts
        `__raea_call` into a context, then web shims for what bundled packages expect and bare V8 lacks: TextEncoder and
        TextDecoder, URL and URLSearchParams (WHATWG, natively parsed), EventTarget, Event, AbortController,
        AbortSignal, atob, btoa, structuredClone, crypto (random values, UUIDs, SHA digests), Blob, Headers, Buffer
        (bytes and strings) and WritableStream (in-order writes to a sink), each only where the context lacks it
      And the same binding is in raea-react's V8 (raea-motion, the iPhone apps) and in the engine's page runtime
        (blitz-v8-script, HostCore's composition scripts)
    Scenario: Isolates with fewer rights
      Then a thread can limit the calls its isolate may make (names or "prefix.*"); a call outside the list throws
        "<name>: not allowed here" (js-exec's isolates get the web shims' calls only)

  Rule: Bundling and running
    Scenario: The bundler (react/js/bundle.ts, run by raea-motion through bun on the Mac)
      Then a script is bundled where it lives (its own node_modules resolve); @raea/motion, @raea/shell and
        @raea/browser resolve to their sources; react, mediabunny and bevy-react resolve to the engine's single copies;
        `bun:test`, `vitest` and `@jest/globals` are the engine's test API; node:zlib and node:path are raea's; another
        Node built-in fails the bundle naming its importer, unless --node-stub lists it; --alias and --define pass
        through
      And a composition's every source (not just the entry) has its @raea/motion imports pointed at the one bundled
        copy, so a component's hooks read the Clock the engine drives
    Scenario: Running
      Then `raea-motion run SCRIPT.ts` runs a script's default export on V8 inside Bevy; `raea-motion test FILE...`
        runs bun:test-style files there and prints a PASS or FAIL line per case; react/ios/test.sh runs the same files
        on the iPhone (the probe app, its report written after each item)
      And the agent's own modules load without bun through raea-modules (deno_ast): transpiled in process, imports
        resolved inside the space, run as ES modules (js-exec today; the prebuilt map for bare names is next)

  Rule: What TypeScript reaches (each specified in its own feature)
    Scenario: The libraries
      Then @raea/motion/ecs is Bevy's own World (motion/spec/motion.feature, the ECS Rule); /3d's manifold and Solid
        are Manifold in pure Rust (the solids Rule); /codecs is mediabunny on the engine's coders (the codecs Rule)
      And @raea/shell is just-bash over the project folder with raea's tools (shell/shell.feature); @raea/browser is
        agent-browser in process with a Blitz backend (browser/conformance, the refs identical to Chrome's)

  Rule: The proof
    Scenario: The same answers everywhere
      Then the conformance suites pass under bun and on the engine's V8 with the same expectations; the shell's probe
        command set gives byte-identical output on the Mac and the iPhone (shell/conformance/probe-golden.json)
      And outside references agree: media against real ffmpeg (motion/tools/media-vs-ffmpeg.sh: the H.264 take 22/22,
        the rotated mono HEVC take 29/29), the browser's Blitz snapshots against Chrome's line for line, just-bash's
        own test suite on our V8 (shell/tools/upstream.sh)
