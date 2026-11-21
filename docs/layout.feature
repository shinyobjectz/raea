Feature: One project layout: compositions, components, assets, artifacts; and the workspace's shared/
  Shane, 11-12: "reorganize the default project organization to remove "parts" and really align around the folders of
  compositions, components, controllers, assets, artifacts. this should be the default, and at the workspace level
  having a library that any project can call upon from those global folders." On naming: "just make sure it doesn't
  get confused with our other concept of libraries" (docs/libraries.feature: indexed media, `raea library`), so the
  workspace's folder is shared/ and its import prefix @shared/. Existing projects move with a command, reviewed
  before they land; `raea part` becomes `raea component`.

  Read 11-12, before any change:
    - `raea project new` (apps/workspace/src/workspace.rs new_project) makes brief.ts, AGENTS.md, media/, brand/, out/.
      Compositions and components are loose .tsx at the project's root, told apart by `raea project scan` (PJ8).
    - Agents made parts/ by hand in stay and orientation (scene pieces); nothing scaffolds it. "Parts" is otherwise
      raea's own sets: `raea part add reel` copies examples/parts/reel into components/reel/ (engine/cli index.ts,
      registry.gen.ts from tools/pack.ts).
    - The workspace root has controllers/ (PJ2) and artifacts/; 11-07 (docs/organize.md) made controllers per project.
    - The engine's loader (engine/modules resolve) takes relative imports inside the space and prebuilt libraries by
      name; anything else is refused (UnknownLibrary, OutsideSpace). PJ8's lint flags a component imported from
      outside the project's folder.

  Built 11-12 (lane layout): LY1 (raea project new and workspace init make the folders, .gitattributes, AGENTS.md
  and the @shared/ tsconfig), LY2 (the loader resolves @shared/, PJ8's lint takes it), LY3's scan and lint and
  controllers over shared/controllers/, and LY4's `raea component` (`raea part` still answers). Still to do: LY3's
  app pages (Compositions, Controllers, Source), LY4's examples/components rename and the guides and skill wording,
  LY5 (`raea project tidy`), LY6.

  11-20 (projects.feature PJ9): controllers are gone, so a new project or shared/ has four folders, not five, and a
  project's or workspace's rules are its taste.ts. The lines below that name controllers are kept as they were planned.

  The layout:

      <workspace>/
        .raea/workspace, taste.ts, projects/, attachments/   as now
        shared/                      what every project in the workspace can call on
          compositions/  components/  assets/  artifacts/
      <workspace>/projects/<name>/
        .raea/project, brief.ts, <name>.script.ts, AGENTS.md   what the project is (as now, at its root)
        compositions/    what renders: a registerRoot <Composition>, or a default export; versions NAME-v2.tsx
        components/      what compositions import: props as rigs (component-library-first), raea's sets
        assets/          media in: footage, stills, audio, models, fonts, the brand kit (assets/brand/)
        artifacts/       the small React apps agents post to the feed
        out/             renders (LFS, as now)

  Scenario: LY1. A new project and a new workspace have the layout
    When `raea project new NAME` makes a project
    Then its folder has compositions/, components/, assets/ and artifacts/ (each kept with a .gitkeep),
      out/, brief.ts and AGENTS.md, and no media/, brand/ or parts/
    And `raea workspace init` makes shared/ with the same four folders, and no root controllers/ or artifacts/
    And .gitattributes keeps assets/** and out/** in LFS (shared/assets/** too)
    And AGENTS.md says where each kind of file goes, in a line each, and that @shared/ is the workspace's
    And `raea init` in a loose folder makes compositions/edit.tsx and components/, not edit.tsx at the root

  Scenario: LY2. A project imports from the workspace's shared/ by @shared/
    Given shared/components/desk.tsx in the workspace
    When a project's composition imports "@shared/components/desk"
    Then the engine's loader resolves it to the workspace's shared/components/desk.tsx (walking up to .raea/workspace),
      as it resolves a relative import, with the same extensions and index files
    And a render, `raea check`, `raea context` and the app's Compositions page all read it the same way: the bundler
      that copies an edit (runtime/bundle outside.rs) copies what it imports from shared/ beside it, and the one that
      bundles a script where it lives (engine/bundle script.rs) resolves it from the importer (raea_io_home::shared).
      0.1.7's render left @shared/ to the bundler, which took it for a package: "Can't find variable:
      _shared_components_badge" before the first frame, though tsc passed and raea's skill says to import this way
    And TypeScript resolves it too: the project's tsconfig (made by `raea project new`, kept by `raea doctor --fix`)
      maps @shared/* to ../../shared/*, so an editor and tsc follow the import
    And @shared/ outside a workspace is refused with what to do (UnknownLibrary's message names shared/)
    And PJ8's lint no longer flags an @shared/ import; a relative import that climbs out of the project still is,
      with @shared/ as the fix when the file is in shared/

  Scenario: LY3. One reading of the layout, everywhere
    When `raea project scan`, the app's Compositions and Source pages, and `raea context` read a project
    Then compositions come from compositions/ and components from components/, each also from shared/ (marked shared)
    And a .tsx elsewhere in the project is still read by what it is (PJ8), and linted as out of place with the folder
      it belongs in as the fix
    And Source's Assets tab lists assets/ (and shared/assets/), and its Artifacts tab artifacts/ (and shared/artifacts/)
    And an agent's feed_post of an artifact writes it to the project's artifacts/

  Scenario: LY4. `raea component` replaces `raea part`
    When `raea component list` or `raea component add reel [--shared]` runs
    Then it lists and copies raea's sets as `raea part` did, into components/<set>/ (or shared/components/<set>/)
    And examples/parts is examples/components, and registry.gen.ts is packed from it
    And `raea part ...` still works for this release, saying once that it's `raea component` now
    And the guides, the raea skill and AGENTS.md say components, never parts

  Scenario: LY5. An existing project moves into the layout with one command
    When `raea project tidy [NAME] [--dry-run]` runs on a project in the old layout
    Then each file goes where its kind belongs, as the scan reads it: compositions (and their versions) to
      compositions/, the .tsx they import to components/ (parts/ and its subfolders as components/<same path>),
      media/ and brand/ to assets/ (assets/brand/), artifacts to artifacts/
    And brief.ts, scripts, notes (.md) and out/ stay where they are; anything it can't place is listed, not moved
    And every import that names a moved file is rewritten (relative paths, media imports), and so are the paths the
      project's feed items, render manifests and checks name
    And --dry-run prints the moves and rewrites and changes nothing
    And it proves the move before it commits: the scan has no new lint, tsc passes if it passed before, and each
      composition's first frame renders as it did (raea check --quick); a failure undoes the move
    And it is one commit per project, moves as renames (git mv), so history follows each file
    And the workspace's root artifacts/ move to shared/ the same way (`raea workspace tidy`)

  Scenario: LY6. The Raea workspace is tidied
    Given the Raea workspace (~/Documents/raea/raea): stay, orientation, v0-1-launch and two untitled projects
    When each is tidied, its --dry-run read by Shane first
    Then each project's feed still opens with its items playing, its compositions render, and nothing is lost
    And `raea doctor` in an old-layout project says to run `raea project tidy`
