Vendored fontique (from parley git rev 62790943, Apache-2.0 OR MIT) for raea, patched in through the
root Cargo.toml's `[patch."https://github.com/linebender/parley"]`. Local changes:
- src/backend/coretext.rs (macOS only): installed families take CoreText's view, as Chrome 152 matches them:
  weights from kCTFontWeightTrait through Apple's weight constants, faces in CoreText member order. iOS keeps
  fontique's own order and weights (CTFontCollectionCreateMatchingFontDescriptorsForFamily is macOS-only).
- src/font.rs: Blink's synthetic-bold rule (request >= 600 on a face < 600); `set_weight`.
- src/font.rs, src/charmap.rs, src/collection/query.rs: `FontInfoOverride::unicode_ranges` (`@font-face`
  `unicode-range`): a face's Charmap maps nothing outside it, and the first available font must cover U+0020.
- src/font.rs, src/collection/query.rs: `FontInfoOverride::order` (definition order); faces of a family with the
  same descriptors are tried last-defined first, as CSS requires, whatever order their files loaded in.
- src/font.rs, src/matching.rs: `FontInfoOverride::weight_range` (`font-weight: 100 900`): a range face matches
  any weight inside it (its nearest end otherwise), draws the request clamped into the range, and loses exact
  ties to single-weight faces.
- src/family.rs: `FamilyInfo::with_fonts`; src/scan.rs: `english_or_first` is pub(crate).
