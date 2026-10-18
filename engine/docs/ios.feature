@owner:unicorn @ticket:T-048
Feature: raea on iOS
  Status: feasibility spike (framework-ios-spike). The renderer builds as a static library for iOS devices and the
  iOS Simulator, links into an app through a C API, and renders the same frames as the CLI. This document covers
  what works on iOS, what does not, and every cfg or feature the spike introduced, so the changes can be merged into
  the main crate.

  Rule: The xcframework builds from one script, and the macOS CLI is unchanged
    The script puts rustup's `~/.cargo/bin` first on PATH, because Homebrew's rustc has no iOS standard library.
    The minimum iOS version is 16.0 (`IPHONEOS_DEPLOYMENT_TARGET`).

    Scenario: Building the xcframework
      When one of these commands runs in framework/:
        """
        scripts/build-xcframework.sh                  # iOS set: no network, no skia, no ffmpeg
        RAEA_FEATURES=skia,network scripts/build-xcframework.sh
        RAEA_MACOS=1 scripts/build-xcframework.sh       # add a macOS arm64 slice (the Mac app)
        """
      Then the output is `target/xcframework/raea.xcframework`
      And it holds a device arm64 slice and a simulator arm64 slice
      And each slice has `raea.h` and a module map that autolinks the system libraries, so Swift can `import raea`

    Scenario: The macOS CLI still builds with every default feature
      When `cargo build --release` or `cargo install --path .` runs
      Then it still builds `raea` with every default feature
      And it renders byte-identical frames

  Rule: The C API is include/raea.h
    It is implemented in `src/ffi.rs`, over the Rust API in `src/api.rs`.

    Scenario: The calls
      Then the C API has these calls:
        | call                                                         | what                                                                                                  |
        | `raea_probe(path, opts_json) -> char*`                         | the CLI's `probe` JSON (size, fps, duration, frames, videos, the audio plan, warnings)                |
        | `raea_lint(path, opts_json) -> char*`                          | the CLI's `lint --json`                                                                               |
        | `raea_snapshot(path, t, opts_json, out, len) -> status`        | one frame                                                                                             |
        | `raea_render_begin(path, opts_json) -> raea_render*`             | probe and plan a render of `[start, end)`                                                             |
        | `raea_render_info(h) -> char*`                                 | the probe JSON plus `firstFrame`, `endFrame`, `frameCount`, `workers`, `bytesPerFrame`                |
        | `raea_set_video_frame_provider(h, cb, user_data)`              | host-decoded `<video>` frames (set before the first frame)                                            |
        | `raea_render_next_frame(h, buf, len) -> index / RAEA_DONE / error` | the next frame, packed rows                                                                         |
        | `raea_render_next_frame_strided(h, buf, len, row_bytes)`       | the next frame into, for example, a locked CVPixelBuffer                                              |
        | `raea_render_frame_at(h, t, buf, len, row_bytes)`              | any frame in any order (scrubbing); a separate worker keeps the composition loaded                    |
        | `raea_render_boxes_at(h, t, safe_zone) -> char*`               | text-layout JSON at `t` (the CLI's `snapshot --boxes` sidecar); `safe_zone` is "T,R,B,L" or NULL      |
        | `raea_render_end(h)`                                           | stop the workers and free the handle                                                                  |
        | `raea_last_error()`, `raea_free_string()`, `raea_version()`        |                                                                                                       |

    Scenario: Options mirror the CLI flags
      Given an `opts_json` object
      Then its keys mirror the CLI flags:
        | key           |
        | `fps`         |
        | `scale`       |
        | `width`       |
        | `height`      |
        | `start`       |
        | `end`         |
        | `variables`   |
        | `backend`     |
        | `transparent` |
        | `workers`     |
        | `network`     |
        | `videoFrames` |
        | `pixelFormat` |
      And `pixelFormat` is `"rgba"` or `"bgra"`, which the CLI does not have
      And unknown keys are errors

    Scenario: Frames
      Frames are 8-bit premultiplied sRGB (what both rasterizers produce), top row first, written into a buffer the
      host allocates. The swizzle runs on the worker threads.

      Given `"pixelFormat": "bgra"`
      Then frames come in kCVPixelFormatType_32BGRA byte order
      When the host calls `raea_render_next_frame_strided` with a locked CVPixelBuffer's base address and `bytesPerRow`
      Then a pixel-buffer-pool buffer is filled directly, with one copy from the worker's buffer and no intermediate Swift buffer

    Scenario: Video in, through the frame provider (the iOS path)
      The callback is `cb(user_data, video_id, frame_index, media_time, raea_video_frame*)`. `frame_index` is clip-local
      at the render fps (0 is the clip's first output frame, the same numbering as `videoFrames`), and
      `media_time = mediaStart + frame_index / fps * playbackRate` seconds into the source (loop clips wrap by the
      asset duration).

      Given a frame provider set with `raea_set_video_frame_provider`
      When the library needs a `<video>` frame
      Then the host writes RGBA or BGRA rows with any `row_bytes`, so an AVAssetReader BGRA CVPixelBuffer can be memcpy'd as is
      And if the buffer is too small, the host sets width, height and row_bytes, returns `RAEA_FRAME_NEED_BUFFER`, and is called again
      And the size is remembered per video, so that happens at most once per video per worker
      And `videoFrames` (`DIR/<videoId>/frame_%06d.jpg` or `.png`) still works, and is what `raea_snapshot` uses

    Scenario: Audio is never rendered by the library
      Given a composition with audio
      Then `raea_probe`'s `audio` array is the mix plan for AVMutableComposition and AVAudioMix
      And each entry has `src, start, end, mediaStart, volume, loop, playbackRate, fadeIn, fadeOut`
      And videos with `data-has-audio="true"` appear there too

    Scenario: Threading
      A composition holds a Boa context, which is `!Send`, so a composition never leaves the thread that created it.
      Boa and Stylo recurse deeply, and iOS gives the main thread 1 MB of stack and other threads 512 KB.

      Given any call
      Then it can be made from any thread, including the main thread, but every call blocks, so keep them off the main thread
      And `raea_probe`, `raea_lint` and `raea_snapshot` load the composition on an internal thread with a 64 MB stack and return plain data
      And a render handle owns `workers` threads (64 MB stacks, mostly untouched virtual memory)
      And each worker loads its own document and JS runtime and renders frames `k, k + workers, ...`
      And `raea_render_next_frame` hands the frames back in order, with one finished frame queued per worker
      And the frame provider is called on those worker threads, up to `workers` at once
      And a handle is used from one thread at a time, and `raea_render_end` is not called while another call on it is in flight

    Scenario: Workers
      Frames are dealt round-robin (worker k renders k, k + N, ...), so the slowest worker sets the pace: more workers
      than performance cores can be slower, not faster. Memory grows per worker (see the numbers below).

      Given no `"workers"` key in `opts_json`
      Then the default is `min(cores / 2, 2)` on iOS and `min(cores / 2, 8)` elsewhere
      And setting `"workers"` in `opts_json` chooses the count
      And workers are created by the first `raea_render_next_frame` and inherit that thread's QoS, so call it from a `.userInitiated` queue or thread

    Scenario: A Swift sketch (untested; it shows the intended shape)
      Given this Swift:
        """swift
        import raea
        import AVFoundation

        let r = raea_render_begin(projectDir.path, #"{"workers":2,"pixelFormat":"bgra"}"#)!
        defer { raea_render_end(r) }
        let infoPtr = raea_render_info(r)!; let info = String(cString: infoPtr); raea_free_string(infoPtr)

        // <video> frames from AVAssetReader (a class you write; it must be thread-safe)
        let source = Unmanaged.passRetained(VideoFrameSource(project: projectDir))   // release after raea_render_end
        raea_set_video_frame_provider(r, { user, id, index, mediaTime, frame in
            let src = Unmanaged<VideoFrameSource>.fromOpaque(user!).takeUnretainedValue()
            // copy a BGRA CVPixelBuffer: frame.pointee.width/height/row_bytes = its size and bytesPerRow,
            // pixel_format = RAEA_PIXEL_BGRA8; return RAEA_FRAME_NEED_BUFFER if row_bytes*height > capacity
            return src.write(videoID: String(cString: id!), mediaTime: mediaTime, into: frame!)
        }, source.toOpaque())

        while true {   // off the main thread
            var pb: CVPixelBuffer?
            CVPixelBufferPoolCreatePixelBuffer(nil, adaptor.pixelBufferPool!, &pb)
            CVPixelBufferLockBaseAddress(pb!, [])
            let n = raea_render_next_frame_strided(r, CVPixelBufferGetBaseAddress(pb!)!.assumingMemoryBound(to: UInt8.self),
                                                 CVPixelBufferGetDataSize(pb!), CVPixelBufferGetBytesPerRow(pb!))
            CVPixelBufferUnlockBaseAddress(pb!, [])
            if n == Int64(RAEA_DONE) { break }
            if n < 0 { throw UVError(String(cString: raea_last_error())) }
            while !writerInput.isReadyForMoreMediaData { usleep(1000) }
            adaptor.append(pb!, withPresentationTime: CMTime(value: n, timescale: CMTimeScale(fps)))
        }
        // audio: build an AVMutableComposition + AVAudioMix from info["audio"]
        """
      Then the host renders frames into an AVAssetWriter and builds the audio from the probe's plan

  Rule: What works on iOS, and what doesn't
    The iOS build is `--no-default-features`.

    Scenario: The iOS build, area by area
      Then the iOS build behaves like this:
        | area                                              | iOS build (`--no-default-features`)                                                                                                                                                                                                                                                                                                                                                                                                                                     |
        | layout, paint, GSAP, scripts                      | the same code as macOS; Boa is an interpreter, so no JIT is needed                                                                                                                                                                                                                                                                                                                                                                                                      |
        | raster                                            | vello_cpu always; Skia with `--features skia` (prebuilt Skia binaries exist for `aarch64-apple-ios` and `-ios-sim`). With `skia`, vello also draws text from Skia's CoreText glyph masks (Chrome's text raster); without it, text is outlines (small text is ~5-8% lighter than Chrome)                                                                                                                                                                                  |
        | local assets                                      | `file:` (the project dir) and `data:` URLs                                                                                                                                                                                                                                                                                                                                                                                                                              |
        | remote assets (Google Fonts `<link>`, CDN images) | not fetched unless built with `network`; they load only from the cache (`$HOME/Library/Caches/raea`, which is the app container's Caches). The composition falls back to system fonts, so text can reflow. Prefer bundled or local fonts on iOS, or ship `network`                                                                                                                                                                                              |
        | `<video>`                                         | the frame provider or `videoFrames`; no ffmpeg. Without either, videos render empty (with a warning in `raea_render_info`)                                                                                                                                                                                                                                                                                                                                                  |
        | audio                                             | the host mixes the probe's plan                                                                                                                                                                                                                                                                                                                                                                                                                                         |
        | encoding (mp4/mov/webm)                           | the host (AVAssetWriter); the library only produces frames                                                                                                                                                                                                                                                                                                                                                                                                              |
        | system fonts                                      | CoreText via fontique, the same backend as macOS (it enumerates CoreText descriptors and scans `*/Library/Fonts`). iOS has the common web families the Mac has (Arial, Helvetica, Times, Georgia, Verdana, Courier, Menlo, Futura, Avenir…), but not Apple Chancery (fontique's `cursive` default), nor anything a Mac user installed into `/Library/Fonts`; the system UI font is SFUI rather than SFNS, so `system-ui` may resolve to a different face. Compositions that rely on a Mac-only or user-installed font render with a fallback on the phone |
        | `fetch()` from scripts, WebGL, canvas             | not supported (the same as macOS)                                                                                                                                                                                                                                                                                                                                                                                                                                       |

  Rule: Numbers were measured on 2026-10-08 on an M4 Mac mini through the C API
    The machine was an M4 Mac mini (10 cores, arm64), measured through the C API (`tools/capi/raea_bench`) with a
    release build. It was shared with other heavy jobs (load average 60 to 470), so fps is noisy; CPU ms per frame
    and memory are not affected by that.

    Scenario: Size, linked into a test executable for iOS arm64 with -dead_strip (what an app pays)
      Then the sizes are:
        | build                                           | `__TEXT` | `__DATA_CONST` | stripped executable | gzip -9 |
        | iOS minimal (vello only, no network, no ffmpeg) | 25.1 MB  | 2.2 MB         | 28.8 MB             | 10.7 MB |
        | iOS + `skia` + `network`                        | 28.1 MB  | 2.3 MB         | 32.1 MB             | 12.3 MB |
        | macOS arm64, all defaults (reference)           | 28.4 MB  | 2.3 MB         | 32.7 MB             | 12.6 MB |
      And `skia` and `network` together add about 3.2 MB (Skia ~1.5 MB after dead-stripping; ureq, rustls, ring and the webpki roots ~1.5 MB)
      And the main crate's bundled OFL webfonts (`fonts/`, ~745 KB of woff2 embedded by build.rs) add ~0.75 MB of incompressible data on top
      And the static library per slice is ~81 MB, because it holds every dependency's code before dead-stripping

    Scenario: Where the minimal build's code goes (link map, by crate)
      Then the code splits like this:
        | crates                                                                                              | size     |
        | Boa (engine, parser, AST, regress, temporal)                                                        | ~5 MB    |
        | Stylo (style, selectors, cssparser)                                                                 | ~3.5 MB  |
        | the font stack (skrifa, read-fonts, harfrust, parley)                                               | ~1.5 MB  |
        | the remainder: generics (core/alloc/hashbrown), taffy, usvg, image codecs, blitz, vello, html5ever, ICU data | the rest |

    Scenario: Estimated app-size impact
      The App Store encrypts executable code, which keeps it from compressing, so budget close to the installed size
      for the download too. The 11 MB gzip figure is the lower bound.

      Given the minimal build plus the bundled fonts
      Then the installed app grows by about 30 MB

    Scenario: Speed and memory at 1080x1920
      Peak memory is `phys_footprint` (the figure iOS's jetsam enforces), per process, including the library's frame
      buffers. "take" is the 1080x1920 talking-head composition (a `<video>`, 18 word-timed captions, a badge, Google
      Fonts Inter), 300 frames. "+provider" decodes the video frames in the frame-provider callback (ImageIO JPEG
      decode, ~15 ms per frame on the worker thread, standing in for AVAssetReader). "+videoFrames" decodes JPEG frames
      inside the library. product-promo is the 1920x1080 the HTML composition format example (the same pixel count, no video, 600
      frames).

      Then the measurements are:
        | composition       | workers | fps | CPU ms / frame | peak memory | load (begin) | first frame |
        | take +provider    | 1       | 44  | 19             | 104 MB      | 120 ms       | 163 ms      |
        | take +provider    | 2       | 80  | 21             | 153 MB      | 104 ms       | 173 ms      |
        | take +provider    | 4       | 105 | 31             | 343 MB      | 125 ms       | 213 ms      |
        | take +videoFrames | 1       | 50  | 20             | 104 MB      | 99 ms        | 100 ms      |
        | take +videoFrames | 2       | 101 | 20             | 173 MB      | 102 ms       | 103 ms      |
        | take +videoFrames | 4       | 88  | 31             | 321 MB      | 124 ms       | 149 ms      |
        | product-promo     | 1       | 31  | 32             | 69 MB       | 286 ms       | 229 ms      |
        | product-promo     | 2       | 44  | 44             | 134 MB      | 222 ms       | 423 ms      |
        | product-promo     | 4       | 67  | 52             | 196 MB      | 505 ms       | 477 ms      |

    Scenario: Memory per worker decides the worker count on a phone
      Memory per worker is roughly +50 to +80 MB per extra worker at 1080x1920 (60 to 100 MB for the first). `begin`
      loads the composition once more on its own to probe it; that copy is dropped before the workers start, so it
      adds load time, not peak memory.

      Given a render worker
      Then it holds its own:
        | what                  | notes                                                                                                                                   |
        | document              | DOM, Stylo style data, Taffy layout, the GSAP animation scene                                                                            |
        | Boa runtime           | with the GSAP shim and the composition's scripts                                                                                        |
        | font collection       | Blitz builds a non-shared fontique collection per document, so each worker indexes the system fonts and decodes the web fonts itself  |
        | decoded images        | including the current video frame (width x height x 4 = 8 MB at 1080x1920)                                                              |
        | vello_cpu render state |                                                                                                                                        |
        | frame buffers         | up to three of 8 MB (one rendering, one queued for the host, one recycled)                                                              |

    Scenario: Reading the numbers for iPhone
      At 1080x1920, 2 workers peak around 150 to 175 MB, far below the jetsam limit of any recent iPhone (multiple GB on
      6 to 12 GB devices), and even 4 workers stay under 350 MB. CPU time is ~20 ms per frame per worker on an M4
      P-core; an A17 or A18 Pro P-core is in the same class, and the E-cores are 3 to 4 times slower. That is an
      extrapolation; it has not been run on a device.

      Given an iPhone with an A17 or A18 Pro
      When 2 workers render this kind of composition on the two P-cores
      Then it should render at or faster than real time (30 fps), with thermals as the limit on long renders

  Rule: Blockers found, and what was done

    Scenario: 1. num_threads does not compile for iOS (the only build blocker)
      boa_engine pulls in `time` with local-offset support, which uses `num_threads`; its Apple code calls
      `libc::mach_task_self()`, and libc 0.2.190 now binds that on macOS only. Upstream fix candidates: libc
      re-enabling it for iOS, or num_threads switching to `mach2`.

      Then num_threads 0.1.7 is vendored (`vendor/num_threads`, `[patch.crates-io]`) with the symbol `mach_task_self_` (present on every Apple OS) declared directly
      And the behaviour on macOS is the same

    Scenario: 2. Toolchain
      Homebrew's `rustc` (first on PATH on this Mac) has no iOS standard library.

      Then builds must use rustup's toolchain (`~/.cargo/bin` first on PATH), which the script does

    Scenario: 3. Boa is no issue
      Boa is a tree-walking and bytecode interpreter with no JIT.

      Then it runs under iOS's no-JIT rule unchanged

    Scenario: 4. fontique system fonts are no issue
      fontique's CoreText backend is selected by `target_vendor = "apple"`.

      Then iOS uses the same code as macOS (see the system fonts row above)

    Scenario: 5. ICU (text-transform-icu) is no issue
      Then icu4x runs with compiled-in data, in pure Rust

    Scenario: 6. ureq, rustls and ring cross-compile cleanly for iOS
      ring's assembly has iOS variants. Shipping a TLS stack for Google Fonts is a product choice (it adds about
      1.5 MB).

      Then they are still behind the default-on `network` feature
      And with `network` off, http(s) loads only from the cache

    Scenario: 7. Skia is not a blocker
      rust-skia publishes prebuilt binaries for `aarch64-apple-ios` and `aarch64-apple-ios-sim` with this crate's
      feature set (ganesh, metal, pdf, jpeg), and they link.

      Then Skia is behind the default-on `skia` feature, so the iOS build can ship vello-only
      And Skia adds about 1.5 MB after dead-stripping, and pulls in UIKit, Metal, ImageIO and MobileCoreServices

    Scenario: 8. Blank frames when a stylesheet fails to load (a runtime blocker, pre-existing, not iOS-specific)
      This was found by running in the Simulator, where the take's Google Fonts `<link>` cannot load.

      Then it is fixed in net.rs (see the src/net.rs entry in the merge checklist)

    Scenario: 9. ffmpeg
      ffmpeg is used by `media.rs` for frame extraction, and by `main.rs` for encoding and the audio mix.

      Then it is behind the default-on `ffmpeg` feature, which is off on iOS
      And on iOS, `<video>` frames come from the host (the frame provider or `videoFrames`)
      And encoding and the audio mix are the host's (AVAssetWriter and AVAudioMix)
      And main.rs's encoder is CLI-only code and is not part of the library

  Rule: The cfg and feature changes (merge checklist)
    Every change is additive or a small gate; no existing code was reformatted. Search for `feature = "` in src/ to see
    all gates. No `cfg(target_os = "ios")` gates were needed for the build; the only one is in `api::default_workers`
    (it caps the default worker count at 2 on iOS).

    Scenario: Cargo.toml
      The xcframework script does not rely on the `staticlib` entry: it runs `cargo rustc --crate-type staticlib`,
      because when rustc emits the staticlib together with the rlib, the archive also carries every dependency's
      embedded LLVM bitcode (336 MB instead of 81 MB for the same linked code; Xcode 26's `bitcode_strip` cannot clean
      archives). With `staticlib` listed, every CLI `cargo build` or `cargo install` also writes that ~370 MB archive
      to target/release, so `crate-type = ["rlib"]` is the leaner choice for the main crate.

      Then `[lib] name = "raea", path = "src/lib.rs", crate-type = ["staticlib", "rlib"]` is added
      And `[features] default = ["network", "skia", "ffmpeg"]` is added, with these features:
        | feature                       | change                                                                 |
        | `network = ["dep:ureq"]`      | ureq, rustls, ring, webpki-roots; `ureq` became `optional = true`      |
        | `skia = ["dep:anyrender_skia"]` | `anyrender_skia` became `optional = true`                            |
        | `ffmpeg = []`                 |                                                                        |
      And `[patch.crates-io] num_threads = { path = "vendor/num_threads" }` is added (see blocker 1)

    Scenario: src/lib.rs (new)
      Then it has `pub mod` for every module, plus `api` and `ffi`
      And in the main crate it also needs `pub mod fonts;` (the bundled-font module that landed after this copy was made)

    Scenario: src/main.rs
      api.rs carries copies of main.rs's `parse_fps`, `resolve_input`, `frame_count`, `default_fps`, `probe`,
      `probe_json` and the lint body, so main.rs stayed untouched while other work lands in it. A follow-up can make
      main.rs call api.rs and delete its copies.

      Then the `mod ...;` lines became `use raea::{media, runtime, scene};`
      And nothing else changed

    Scenario: src/net.rs
      The bug fix affects the CLI too. Blitz blocks all rendering until every render-blocking request (a
      `<link rel=stylesheet>`) responds, so one unanswered failure rendered the whole frame blank: the CLI with
      `--no-network` and a cold cache (or offline) rendered a white frame for any composition with a Google Fonts
      `<link>` (reproduced with the installed CLI), and every such composition was blank on iOS without `network`.

      Then the ureq request moved into `fn http_get`, with a `#[cfg(not(feature = "network"))]` stub that errors ("not cached, and this build has no network support")
      And `USER_AGENT` is `#[cfg(feature = "network")]`
      And file:, data: and the on-disk cache are unchanged
      And `NetProvider::fetch` now answers a failed load with empty bytes instead of dropping the handler
      And the stylesheet is treated as empty, so the frame renders with fallback fonts
      And output is unchanged whenever loads succeed

    Scenario: src/media.rs
      Then `use std::process::Command` is `#[cfg(feature = "ffmpeg")]`
      And `VideoFrames::extract` and `ffmpeg_path` are `#[cfg(feature = "ffmpeg")]`, each with a `#[cfg(not(feature = "ffmpeg"))]` stub
      And without ffmpeg, extract errors and ffmpeg_path returns None, so the CLI's existing "ffmpeg not found" paths report it
      And these are new:
        | item                          | what                                                                                    |
        | `FrameProvider`               | host frames                                                                             |
        | `VideoEntry::provider`        | None for directory or ffmpeg frames                                                     |
        | `VideoFrames::frame_index`    | the clip-local index math, moved out of `frame_path`, which now calls it                |
        | `VideoFrames::frame_rgba`     | provider or file decode                                                                 |
        | `VideoFrames::from_provider`  |                                                                                         |

    Scenario: src/scene.rs
      Then `use anyrender_skia::SkiaImageRenderer`, `Raster::Skia` and its two match arms are `#[cfg(feature = "skia")]`
      And without the feature, `Composition::load` rejects `Backend::Skia` with an error
      And the `Backend` enum and the CLI's `--backend skia` parsing are unchanged
      And `apply_video_frames` keys its "already shown" cache by clip-local frame index (`video_shown: HashMap<String, Option<usize>>`)
      And it gets pixels from `VideoFrames::frame_rgba` instead of decoding `frame_path` directly, with the same frames and the same output

    Scenario: New files
      Then the spike added these files:
        | file                             | notes                                   |
        | `src/api.rs`                     |                                         |
        | `src/ffi.rs`                     |                                         |
        | `include/raea.h`        |                                         |
        | `scripts/build-xcframework.sh`   |                                         |
        | `tools/capi/raea_bench.c`          |                                         |
        | `tools/capi/build.sh`            |                                         |
        | `vendor/num_threads/`            | vendored 0.1.7 with a 4-line fix        |
        | `docs/ios.feature`               | this document                           |
