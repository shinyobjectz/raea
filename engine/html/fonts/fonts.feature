Feature: Bundled fonts for deterministic font aliasing
  This folder holds the bundled fonts used for deterministic font aliasing (see
  src/fonts.rs). They are Fontsource latin subsets, extracted to woff2 files.

  Each family is licensed under the SIL Open Font License 1.1, and the license texts are in licenses/.

  Scenario: A bundled face is found through index.json
    Given index.json, which maps package, weight and style to a file
    When the build embeds the fonts (build.rs reads fonts/index.json)
    Then each listed woff2 file is bundled for src/fonts.rs
