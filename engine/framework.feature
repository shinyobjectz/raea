@owner:unicorn @ticket:T-048
Feature: raea (`raea`) renders the HTML composition format compositions to video without a browser
  raea renders HTML + CSS + GSAP video compositions (the the HTML composition format format) to video without a browser.
  Blitz (`blitz-dom` + Stylo) does styling and layout, Vello (`vello_cpu`) or Skia paints, and GSAP timelines are
  evaluated natively by animato. Composition scripts still run, in Boa through Blitz's DOM bindings, but the `gsap`
  they see is a recorder: the timeline tree goes to Rust and every frame is a pure function of time.

  Install it with `cargo install --path ~/the host app/framework`, which puts `raea` in `~/.cargo/bin`.
  Agents normally reach it through `ugc render|probe|snapshot|lint`.

  Related docs: tracked small diffs are in docs/parity-gaps.feature, and the milestone plan is docs/parity-plan.feature.

  Rule: Each part of the engine lives in one place in the source tree

    Scenario: The source tree
      Given the folder the host app/framework:
        """
        the host app/framework/
          src/main.rs        CLI: probe | render | snapshot | lint, parallel workers, encoding
          src/compile.rs     compile step: sub-composition inlining, CSS + script scoping, GSAP removal, <video> -> frame images
          src/scene.rs       one loaded composition: scripts, per-frame apply (CSS, SVG attributes, DOM props), paint
          src/gsap_shim.js   the `gsap` compositions call: records the authored tree with GSAP's placement rules
          src/gsap.rs        GSAP semantics in Rust: tracks, overwrite, from/to/startAt start values, repeat/yoyo, callbacks
          src/ease.rs        GSAP ease strings -> animato easings
          src/value.rs       CSS values made interpolable (animato `Interpolate`): units, colors, complex strings, calc() mixes
          src/runtime.rs     the HTML composition format runtime timing: data-start expressions, clip windows, visibility
          src/media.rs       <video>/<audio> plan, frame extraction (ffmpeg or caller-provided), audio mix
          src/net.rs         file / data: / http(s) assets, http cached in ~/Library/Caches/raea
          src/fonts.rs       the composition format's deterministic fonts: aliased families -> bundled OFL faces (fonts/), else Google Fonts
          vendor/            patched copies, each with a header naming its change: blitz-paint (text-shadow, mix-blend-mode,
                             background-clip:text, box-shadow sigma, Blink paint-offset snapping
                             and bleed avoidance), blitz-dom (clip-local CSS animation clock),
                             anyrender_skia, anyrender_vello_cpu, vello_common/vello_cpu (speed, Skia-exact 8-bit rounding),
                             parley_engine (Blink metrics), parley (LayoutUnit line boxes)
          bench/parity.sh    parity gate vs the composition format's Chrome (tests/parity fixtures + registry examples), SSIM + within-2/255
          bench/milestones.py per-milestone gate report (docs/parity-plan.feature) from a parity run
        """

  Rule: The CLI has four commands: probe, render, snapshot and lint
    The options are `--fps N|N/D`, `--scale S`, `--width/--height`, `--start/--end`, `--workers N`,
    `--quality draft|standard|high`, `--crf N`, `--variables JSON|@file`, `--video-frames DIR`, `--no-network`,
    `--transparent`, `--backend auto|vello|skia` and `--quiet`.

    Scenario: The commands and what they print
      Given the CLI:
        """
        raea probe    <dir|file.html>                 JSON: size, fps, duration, frames, videos, audio plan, warnings
        raea render   <dir|file.html> -o out.mp4      mp4 | mov (ProRes 4444) | webm (VP9) | png-sequence | rgba
        raea snapshot <dir|file.html> --at 1,2.5 -o out.png|dir
        raea lint     <dir|file.html> [--json]        exit 1 on errors; lists unsupported features
        """
      Then logs go to stderr and JSON goes to stdout

    Scenario: `--backend auto` chooses the painter
      Given `--backend auto`, which is the default
      When the composition uses `backdrop-filter`
      Then Skia paints it
      And otherwise vello_cpu paints it
      And `probe` reports the choice as `backend`

  Rule: Callers that encode the video themselves (the Mac app) rely on a fixed contract

    Scenario: Raw frames on stdout
      When a caller runs `render --format rgba -o -`
      Then raw RGBA8 frames are written to stdout in order, `width*height*4` bytes each
      And `width`, `height`, `fps` and `frames` come from `probe`

    Scenario: The audio mix plan
      When a caller runs `probe`
      Then its `audio` array is the mix plan: `{src, start, end, mediaStart, volume, loop, playbackRate, fadeIn, fadeOut}`

    Scenario: Caller-provided video frames replace ffmpeg decoding
      Given `--video-frames DIR`
      Then frames are read from `DIR/<videoId>/frame_%06d.(jpg|png)`
      And frame 0 is the clip's first output frame, with one image per output frame
      And `videoId` is `probe`'s `videos[].id`

    Scenario: Without caller-provided frames, ffmpeg decodes and encodes
      Given no `--video-frames` option
      Then `<video>` sources are decoded once with ffmpeg into the cache
      And mp4, mov and webm encoding and the audio mix use ffmpeg from PATH (or `$RAEA_FFMPEG`)

  Rule: Coverage is verified against Chrome renders of the the HTML composition format examples

    Scenario: GSAP
      GSAP covers `timeline/to/from/fromTo/set/call/add/addLabel`, position parameters (`<`, `>`, `+=`, `-=`, labels,
      `%`), timeline `defaults`, stagger (`each`/`amount`/`from`, with grid treated as 1-D), `repeat`/`yoyo`/`repeatDelay`,
      keyframes, `startAt`, all core eases plus `back(n)`, `elastic(a,p)`, `steps(n)`, `rough`, `slow`, `expoScale`,
      directional rotation (`"270_cw"`, `_ccw`, `_short`), `clearProps` (including `"all"`), numeric `innerText` with
      `snap`, CustomEase cubic paths and function eases, `overwrite: "auto" | true`, function-based values, relative
      values, plain-object tweens with `onUpdate` (count-ups, typewriters), `onStart`/`onComplete`/`tl.call()`,
      `gsap.set/to` from callbacks (applied live), `gsap.utils`, `registerEffect`, SplitText chars/words, and
      `getVariables()`.
      Given a composition that uses GSAP
      Then those GSAP features render as in Chrome

    Scenario: GSAP plugins, reimplemented natively
      The plugins are reimplemented natively from their algorithms, and no GSAP code ships. DrawSVG is covered.
      MotionPath is covered with `path` (a <path> or path data), `align`, `alignOrigin`, `autoRotate`, `offsetX/Y` and
      `resolution`; not yet covered are point arrays and `start`/`end` slicing. MorphSVG is covered with
      `type: "linear"`, `shapeIndex`, `map` and `precision`; rotational and `smooth` morphs render as linear.
      `FontFace()` + `document.fonts.add()` are covered too.
      Given a composition that uses DrawSVG, MotionPath or MorphSVG
      Then it renders without any GSAP plugin code

    Scenario: CSS
      CSS covers everything Stylo + Blitz lay out (flex, grid, positioning, fonts including Google Fonts and the
      `@font-face` rule, emoji), 2D transforms, opacity, filters including blur, backdrop-filter (Skia), mix-blend-mode,
      background-clip:text, clip-path, gradients, box-shadow, text-shadow, mask-image, and object-fit/object-position.
      Padding and border-radius on inline `<span>`s are not painted; use `display: inline-block` for pills.
      Given a composition styled with CSS
      Then those CSS features render as in Chrome

    Scenario: CSS 3D
      CSS 3D covers 3D transform functions, `perspective`/`perspective-origin`, `transform-style: preserve-3d` (planes
      ordered by a BSP tree like Chrome's compositor, with intersecting planes split) and `backface-visibility`. GSAP
      rotationX/rotationY/z/transformPerspective emit real 3D transforms and start from an element's CSS matrix3d, as
      GSAP does.
      Given a composition with CSS 3D planes
      Then they are ordered and split like Chrome's compositor

    Scenario: the composition format's deterministic fonts
      Fonts follow the composition format's deterministic aliases (Helvetica/Arial/SF -> Inter, Times/Georgia -> EB Garamond,
      Courier -> JetBrains Mono, ...), including its compiler's rewrites: a font stack led by a generic or host-UI
      family (`-apple-system`, `system-ui`, `sans-serif`) gets Inter in front, and Google Fonts families (also those
      named in local linked stylesheets) come from Google's `text=` subsets of the page's characters. `@font-face`
      `unicode-range` and weight ranges (`100 900`) are honoured, overlapping faces resolve last-defined-first, and web
      fonts lose the tables Chrome's sanitizer (OTS) drops (AAT `morx`/`kerx`/`trak`/`feat`, Apple `kern`).
      Given a composition that names a font family
      Then the face is chosen the way the HTML composition format chooses it

    Scenario: Text raster with and without Skia
      With feature `skia`, axis-aligned text is drawn from Skia's glyph masks (CoreText on Apple platforms, like
      Chrome). Color fonts and other transforms use vello outlines. `RAEA_OUTLINE_TEXT=1` forces outlines.
      Given the feature `skia`
      When text is axis-aligned
      Then it is drawn from Skia's glyph masks

    Scenario: CSS @keyframes and transitions
      CSS @keyframes run on the composition format's clip-local clock (`t` minus the start of the closest `[data-start]`), with
      delays, iteration counts, direction and fill modes. CSS transitions are not seeked, as in the HTML composition format.
      Given a composition with CSS @keyframes
      Then they run on the clip-local clock

    Scenario: Inline SVG
      Inline SVG is covered, including GSAP on SVG children (GSAP's matrix + origin semantics) and stylesheet styles on
      SVG children. The SVG geometry API (`getTotalLength`, `getPointAtLength`, `getBBox`) is polyfilled.
      Given a composition with inline SVG animated by GSAP
      Then it renders with GSAP's matrix and origin semantics

    Scenario: The the HTML composition format runtime
      The runtime covers sub-compositions (`data-composition-src`) with scoped CSS and scripts, `data-start`
      expressions, frame-snapped clip visibility, sub-composition timelines nested at their host's start, `<video>`
      (any ffmpeg codec, with rotation honoured), `<audio>`, and variables.
      Given a composition with sub-compositions, clips and media
      Then they are timed as the HTML composition format times them

    Scenario: Scripts in Boa
      Scripts get `fetch()` including `arrayBuffer()`/`blob()`, `XMLHttpRequest` (text/json/arraybuffer/blob, sync and
      async), `new Image()` with `onload`/`decode()`/`naturalWidth`, `MutationObserver`, `ResizeObserver`,
      `IntersectionObserver`, `performance.now/mark/measure`, `Intl` (feature `intl`, on by default; currency and
      percent formatting are patched over Boa's gaps), and Tailwind's browser runtime (`@tailwindcss/browser`). They
      also get Blob/object URLs, `Request`, and `new Event()`/`CustomEvent` with `dispatchEvent` on elements, window and
      document. Inline elements' `opacity` applies to their text and inline backgrounds (word-by-word fades in spans).
      Given a composition script that uses those browser APIs
      Then it runs in Boa

    Scenario: Canvas 2D (feature `skia`)
      The full canvas 2D drawing API is replayed on Skia per frame: paths, arcs, transforms, gradients, compositing
      modes, shadows, clipping, dashes, `fillText` shaped with HarfBuzz rules and the composition format's font aliases,
      `drawImage` of images and other canvases, `putImageData`, and Path2D including SVG path strings. The HTML composition format'
      `hf-seek` event is dispatched every frame (`event.detail.time`), so canvases drawn from it follow the timeline.
      Not yet covered: patterns, `getImageData` pixels, and canvas `filter`.
      Given a `<canvas>` drawn from the `hf-seek` event
      Then it follows the timeline

    Scenario: WebGL 1/2 (feature `webgl`, on by default)
      WebGL calls are recorded in script and replayed on ANGLE over SwiftShader (Vulkan) through EGL, the same
      software GL stack as the composition format's headless Chrome, with Chrome's 4x MSAA drawing buffer. three.js scenes
      (meshes, instancing, GLTF/GLB with embedded textures, EffectComposer bloom) match Chrome pixel for pixel. The
      ANGLE/SwiftShader dylibs (vendor/angle, ~24 MB, from Electron 38.4.0 / Chromium 140) are found via
      `RAEA_ANGLE_DIR`, `<exe>/../lib/raea/angle`, `<exe>/angle`, `<exe>/../Frameworks/angle`, or the
      source tree.
      Given a three.js scene
      Then it matches Chrome pixel for pixel
      But without the ANGLE/SwiftShader dylibs `lint` reports WebGL as unsupported and canvases stay blank

    Scenario: Lottie
      The composition's own lottie-web (SVG renderer) is seeked every frame with the composition format's adapter rules
      (`window.__hfLottie` or `lottie.getRegisteredAnimations()`, the local time of the enclosing composition, loop
      wrap, hold on the last frame). Not yet covered: dotLottie (WASM) players.
      Given a composition with lottie-web
      Then it is seeked every frame

    Scenario: Media and the audio mix
      `<video data-has-audio>` honours `data-volume`, `data-fade-in` and `data-fade-out` in the audio plan (fades are
      clamped to the clip, like the HTML composition format). `data-hf-media-start-basis="global"` keeps a nested media element's
      literal `data-start` on the root timeline. The mix places every clip at its `data-start` (each input padded to
      the render length, then delayed) and puts mono sources in both channels at full level, as the composition format's mixer
      does.
      Given a composition with audio
      Then the mix matches the composition format's mixer

    Scenario: Script errors name where they happened
      When a composition script throws
      Then the error names the file, the inline script and the line:
        """
        compositions/cap.html, inline script 1, line 4:36: TypeError ... | <code>
        """

    Scenario: Not covered yet
      Given a script that calls `getComputedStyle`
      Then it reads animated properties at time 0

  Rule: Snapshots can write a text-layout report next to each PNG
    The C API equivalent is `raea_render_boxes_at(handle, t, safe_zone_or_NULL)`. Text in CSS 3D planes is not reported
    yet.

    Scenario: The boxes report
      When `raea snapshot x.html --at 2,5 --boxes [--safe-zone T,R,B,L]` runs
      Then `<frame>.boxes.json` is written next to each PNG
      And it lists every visible text element in paint order with its box in composition px (transforms included)
      And each entry has `centerOffset` from the canvas centre, effective `opacity`, and the canvas edges it crosses (`clipped`)
      And each entry has `overflowClipped` and `visibleFraction` (masks and overflow reveals)
      And each entry names the safe-zone bands it reaches (`inSafeZone`; the default is 220,140,420,0 at 1080x1920)
      And each entry names the other text boxes it overlaps, with nested text excluded

  Rule: Tests cover the evaluator, and the parity gate covers the pixels

    Scenario: Unit tests
      When `cargo test` runs
      Then it covers eases, values, the GSAP evaluator, timing and compile rules

    Scenario: Visual parity against Chrome
      `bench/parity.sh` renders every case in tests/parity and the registry examples, and compares them with Chrome
      references from `bench/parity-refs.sh` (the composition format's pinned chrome-headless-shell 152, SwiftShader).
      When `bench/parity.sh` runs
      Then the registry examples score these SSIMs against Chrome:
        | examples                                                       | SSIM vs Chrome | note                                                                    |
        | product-promo, decision-tree, swiss-grid, vignelli, warm-grain | 0.993–1.0      |                                                                         |
        | play-mode, nyt-graph                                           | 0.992–1.0      |                                                                         |
        | kinetic-type                                                   | 0.966–0.989    | ±1 compositing rounding under a dark overlay (docs/parity-gaps.feature) |

  Rule: Debug environment variables

    Scenario: What each debug variable prints or changes
      Given these environment variables:
        | variable                                                    | effect                                                                     |
        | `RAEA_DEBUG`                                       | prints the timeline tree and JS calls                                      |
        | `RAEA_DEBUG_APPLY`                                 | prints style writes                                                        |
        | `RAEA_DEBUG_TIMING`                                | prints timing                                                              |
        | `RAEA_DEBUG_EVAL` / `RAEA_DEBUG_PRE_EVAL` | runs JS after or before a snapshot frame; `__blitz_send_message` prints    |
        | `RAEA_DEBUG_GL`                                    | prints every replayed WebGL call                                           |
        | `RAEA_OUTLINE_TEXT`                                | vello draws text from outlines instead of Skia glyph masks, for A/B checks |
