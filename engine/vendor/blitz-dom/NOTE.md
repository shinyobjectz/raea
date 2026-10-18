Vendored blitz-dom for raea. Font-related local changes (search "raea" and the git history of
this folder for the rest):
- src/net.rs: `@font-face` rules carry `unicode-range`, the upper end of a `font-weight` range, and a definition
  order (stylesheet owner node slot, rule index) to fontique.
- src/document.rs + src/util.rs: web fonts drop the tables Chrome's sanitizer (OTS) drops before shaping sees them
  (`drop_unsanitized_tables`: AAT morx/kerx/trak/feat, Apple version-1 kern, anything OTS doesn't know).
- src/util.rs: web fonts also registered in usvg's font database for SVG text (`register_svg_font`).
- src/layout/mod.rs + src/resolve.rs + src/layout/inline.rs: layout keeps LayoutUnit precision (1/64 px) like
  Blink: `round_layout_to` replaces taffy::round_layout's whole pixels, intrinsic inline widths ceil to 1/64.
- src/mutator.rs: clearing an `<img>`'s `src` unloads its image (the element's decoded copy and the document's
  image cache entry), so the engine can show many pictures in turn and hold only the visible ones.
