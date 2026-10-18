Feature: one JavaScript engine for raea: V8 (Deno's rusty_v8), jitless everywhere
  Decided with the user on 2026-10-15, after the setup had become confusing (two engines, two binding crates, JIT on
  some targets, jitless on others). This file is the one place that says what runs where. `raea version` prints it.

  Background: what was measured (M4, 2026-10-15, whole snapshot incl. layout and paint, best of 3)
    #   page                                          Boa       V8 jitless   V8 with JIT
    #   React board, 1000 cards (mount)               1.01 s    0.29 s       0.27 s
    #   2000 cards re-rendered 41 times (reconciler)  44.9 s    3.52 s       1.62 s
    # Size, the Mac `raea` binary stripped: Boa 52.1 MB, V8 79.8 MB (+27.7 MB; +10.3 MB gzip-compressed, the
    # download-size proxy). The iOS device V8 library has its optimizing tiers compiled out (64 MB static vs the Mac's
    # 158 MB), so the iPhone's increase is smaller than the Mac's.
    # Measured on the iPhone 2026-10-15: ugc-core for aarch64-apple-ios, linked into a dead-stripped device executable
    # that keeps every C entry point, then stripped: today's app (network, skia, intl, engine: Boa + its ICU) 144.2 MB,
    # 51.9 MB gzip-compressed; with V8 instead (network, skia, engine, v8; V8 carries its own ICU) 149.5 MB, 54.5 MB.
    # V8 costs the app +5.3 MB linked, +2.6 MB compressed (the download-size proxy): under the ~10 MB line, so it ships.
    # Re-measured 2026-10-15 night on what ships (host agent/dist/HostCore.xcframework, ios-arm64, the V8 build): linked
    # into a dead-stripped arm64 device executable keeping its 33 C entry points (15,874 v8::internal symbols, no
    # boa_engine/boa_gc), `strip -x`: 134.5 MB, 50.2 MB at gzip -9 (below the Boa app's 51.9 MB recorded above; the
    # feature sets differ, so +2.6 MB stays the like-for-like cost of V8).

  Scenario: the engine and its mode
    Given any raea build that runs JavaScript (HTML and React compositions)
    Then the engine is V8, from the `v8` crate (Deno's rusty_v8), through vendor/blitz-v8-script
    And it runs jitless by default on every target: the iPhone, the Mac, Linux, sandboxes
    And the JIT is on only with RAEA_JIT=1, for local benchmarks; nothing shipped or run by an agent sets it
    And an iPhone device can never JIT (Apple allows it only to WebKit; that V8 library has no JIT tiers at all)
    And `raea version` prints "js: v8 (jitless)" (or the JIT line when RAEA_JIT=1), so no one has to guess

  Scenario: why jitless everywhere
    Then the same engine in the same mode behaves the same on every target
    And sandboxes that forbid executable memory (W^X, seccomp, gVisor) run it
    And jitless V8 is 3.5x to 12.7x faster than Boa on our React pages; the JIT adds about 2x more, on heavy re-renders only

  Scenario: Boa is gone
    # Done 2026-10-15, after the iPhone measurement above: vendor/blitz-vibey-script (the Boa bindings) and boa_engine
    # are deleted; the `v8` and `intl` features are gone (V8 is not optional, and V8 has Intl); the shared browser
    # scripts (bootstrap.js, geometry.js, web.js) live in vendor/blitz-v8-script/src/js/. ugc-core's defaults are
    # network, skia, engine, so the app ships jitless V8. The [patch] lines for the Boa crate are gone from rdb, rrl,
    # ios-probe and host/agent.
    Then there is one binding crate, vendor/blitz-v8-script, and no Boa in any build
    And the parity corpus on the V8-only engine is what it was on Boa: 107 exact, 61 ok, 2 fail (react/shadcn's
      font gap), the gate passing against bench/parity-baseline.json (2026-10-15, c8b667a)

  Scenario: rdb runs no JavaScript
    Then rdb builds the engine without a JS engine (features = ["network"]): its captions and titles are rows laid
      out by Blitz, not scripts, and none of this applies to it
