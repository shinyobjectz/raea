Vendored from crates.io `anyrender_skia` 0.12.0 (MIT OR Apache-2.0) for raea.
Local changes: glyph colour alpha kept (was overwritten by brush_alpha), grayscale text edging (headless Chrome parity), synthetic bold via SkFont::setEmbolden,
premultiplied surface + straight-alpha output so transparent renders keep their alpha.
- src/scene.rs: no font hinting (Chrome on macOS renders unhinted; Normal hinting means CoreText font smoothing).
- src/scene.rs: backdrop-filter blur uses mirrored edges (Chrome's SkTileMode::kMirror for backdrop filters).
- src/scene.rs: a backdrop-filter layer with opacity < 1 draws the filtered backdrop at that opacity, then the
  content layer at that opacity on top (Chrome's compositor), instead of one isolated group.
- src/scene.rs: variable-font axes map back to design values exactly via skrifa (upstream interpolated backwards below the default: weight 100 drew at 400).
- src/image_renderer.rs: surface text contrast 0 / gamma 1 (Chrome on macOS blends glyph coverage linearly; Skia's default preblend drew text ~10% lighter).
- src/scene.rs: baseline snapping off (Chrome draws text at fractional baselines; snapping also thickened small text ~6%).
- src/scene.rs: a pure-skew glyph transform (synthetic oblique) becomes SkFont skewX, like Chrome, instead of a canvas concat that also sheared positions.
- src/scene.rs: `SkiaSceneCache::glyph_run_mask` (+ `GlyphMask`): a glyph run's 8-bit coverage drawn exactly as `draw_glyphs` draws it, for the vello backend's text.
- src/scene.rs: a variable font at its default instance still gets every axis set explicitly (as Blink does).
- src/scene.rs: `SkiaSceneCache::path_mask`: a filled path's 8-bit coverage for the vello backend. While it draws, cubics that are elliptical arcs (kurbo's 4/3·tan(θ/4) arms, as CSS border radii are built) go to Skia as conics with weight cos(θ/2), the way SkPath::addRRect/addOval holds Chrome's rounded rects and circles (blend/mix-blend-mode within 2/255 0.988 -> 0.9998). Elsewhere (SVG) only exact circular arcs with standard arms convert (usvg's circles/rounded rects, which Chrome draws as ovals/rrects); authored cubics such as Lottie's 0.5519-arm ellipses stay cubics, as in Chrome.
