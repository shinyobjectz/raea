Feature: Composition scripts on V8
  SUPERSEDED 2026-10-15 by docs/js.feature: V8 is now the only JS engine (jitless everywhere); Boa and the `v8`
  feature are gone. What follows is the port's record (2026-10-13), kept for its parity and speed numbers.
  The DOM bindings (blitz-vibey-script, on Boa) are ported to V8 (the `v8` crate, Deno's rusty_v8) as
  vendor/blitz-v8-script, behind the `v8` cargo feature. Boa stays the default, and the iOS engine (no JIT
  there, and V8 adds ~16 MB). Both crates expose the same `ScriptDocument`; src/scene.rs picks one by feature.

  Background:
    Given `cargo build --release --features v8` builds the V8 binary (default features build Boa)

  Scenario: How the port is shaped
    Then src/js.rs is a Boa-shaped API over V8: JsValue (primitives in Rust, objects as v8::Global),
      JsObject, JsString, Context, JsResult/JsError/JsNativeError, PropertyDescriptor, Attribute,
      ObjectInitializer, FunctionObjectBuilder, NativeFunction, JsArray, at Boa's module paths
    And the DOM natives (src/dom/*.rs, inner_text, timers, state, document) are blitz-vibey-script's,
      with only their imports rewritten, so the two crates stay diffable
    And the active V8 scope is a thread-local set by every entry point (ScriptRuntime::enter and the one
      native trampoline), which is how context-free Boa calls keep their signatures
    And native data (node ids, event flags, selections) is a per-isolate side table freed by weak-handle
      finalizers (300k getComputedStyle + MouseEvent calls: 2.3 s, 58 MB peak)
    And bootstrap.js, geometry.js and web.js are shared with blitz-vibey-script (include_str!)
    And src/v8_prelude.js supplies what boa_runtime did (console to the log crate, URL and URLSearchParams
      over the url crate, TextEncoder/TextDecoder, atob/btoa, structuredClone) and Date on the document clock
    And modules compile through V8 with a resolve callback over the same fetcher and import maps
    And unhandled promise rejections are recorded as "Uncaught (in promise)" after each microtask checkpoint
    And Intl uses Deno's full ICU 74 data (deno_core_icudata); the v8 crate's cut-down chromecast data hung
      in ICU resource loading on the first Intl.RelativeTimeFormat

  Scenario: Parity is the same as Boa (bench/parity.py, 2026-10-13)
    Then the full corpus on V8 is 107 exact, 61 ok, 2 fail, identical to Boa
    And the 2 fails are react/shadcn, the the HTML composition format font gap in docs/react.feature, on both engines
    And react/probe passes 154 of 157 API checks on both engines

  Scenario: Speed (M4, best of 3, whole snapshot including layout and paint)
    Then these hold:
      | page                                         | Boa     | V8     |
      | react board, 300 cards                       | 0.363 s | 0.196 s |
      | react board, 1000 cards                      | 1.138 s | 0.352 s |
      | react board, 4000 cards                      | 4.379 s | 0.837 s |
      | 2000 cards re-rendered 41 times (reconciler) | 66.3 s  | 2.07 s  |
      | react/timeline, 60 frames on 1 worker        | 97 fps  | 185 fps |
      | minimal page                                 | ~0.09 s | ~0.06 s |
    And V8's own profile is spread over its GC and inline caches; no binding hotspot dominates

  Scenario: Builds
    Then the V8 binary is ~75 MB against ~59 MB for Boa
    And the v8 crate downloads its prebuilt static library on the first build (cached by cargo)
