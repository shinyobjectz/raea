# The game library

A searchable reference library for game work: Bevy's own examples with the raea TSX that does the same thing, open
source Bevy games, game-design patterns written down as rules an implementer can follow, and free 3D models, rigs and
animations. Each item is one record that carries its licence. The catalogue, its vector index and the thumbnails ship
with raea (1.8 MB, or 1.0 MB gzipped). Model files are fetched on demand, by licence-checked URL, into
`~/.raea/assets/games/`; they are never bundled into the app or committed, except the one fixture model below.

Built 2026-10-18 by the game-library task. The sources were surveyed on that date.

## Where things are

| Path | What |
|---|---|
| `sdk/motion/library/games/catalogue.jsonl` | the catalogue: 729 records, one JSON object per line (494 KB) |
| `sdk/motion/library/games/index.bin`, `index.json` | the vectors (615 KB) and their metadata, built by `raea-meaning games-index` |
| `sdk/motion/library/games/thumbs/` | 67 thumbnails, 256x256 JPEG (728 KB), each with a known licence |
| `sdk/motion/library/games/tsx/` | 9 raea TSX equivalents, attached to records with `// record: ID` |
| `sdk/motion/library/games/sources/` | the curated records (`core`, `assets`, `games`, `patterns`) and `repos.json`, the GitHub licence cache |
| `sdk/motion/library/games/build.py` | rebuilds the catalogue from Bevy's examples, bevy-assets and `sources/` |
| `sdk/motion/library/games/thumb.py` | renders a thumbnail in raea from a downloadable model, and reports whether it renders |
| `sdk/motion/library/games/pack.sh` | the library bundle (`lib`) and the optional asset pack (`assets`) |
| `ai/meaning/src/games.rs` | `raea-meaning games-find`, `games-show`, `games-fetch`, `games-index` |
| `apps/void/src/brain.rs` | the `game_library` tool (search and fetch) for void's agent |
| `apps/void/prompts/craft/bevy/library.md` | the craft note that tells the agent how to use it (`sdk/skills/raea-bevy/library.md` links to it) |
| `sdk/motion/bench/bevy/character/` | the end-to-end fixture: the CC0 robot found and fetched through the library |

## Sources and licences

Everything below was opened on 2026-10-18. A record is `ship: "download"` only when its licence is CC0-1.0,
CC-BY-3.0/4.0 (with the attribution recorded), MIT, Apache-2.0 or Unlicense, **and** it has a direct file URL.
Everything else is `ship: "link"`: the record describes it and points to it, and `games-fetch` refuses it.

| Source | Records | Licence | Ship |
|---|---|---|---|
| Bevy's examples at v0.19.1 ([examples/README.md](https://github.com/bevyengine/bevy/blob/v0.19.1/examples/README.md)), the categories useful for games | 326 | MIT OR Apache-2.0 | link, with the raea-bevy topic that covers each and 9 TSX equivalents |
| [bevyengine/bevy-assets](https://github.com/bevyengine/bevy-assets) (awesome-bevy was renamed to this; the old URL redirects, and it feeds [bevyengine.org/assets](https://bevyengine.org/assets/)): Apps/Games, Learning, and the Templates, 3D, Animation, Physics, Camera and Input crates | 228 | the list has no licence file; each entry's own repo licence is looked up through the GitHub API (`sources/repos.json`) | link |
| Open-source Bevy games, curated (Jumpy, Punchy, Digital Extinction, Foxtrot, bevy_new_2d, Thetawave, Kataster, Bevy Jam winners #1, #4, #5 and #6, extreme_bevy, vx_bevy ...) with the design patterns each shows | 42 | mostly MIT OR Apache-2.0; Digital Extinction AGPL-3.0; four with no licence file (all rights reserved); Tiny Glade proprietary | link |
| Game-design references (Juice it or lose it, The Art of Screenshake, Eiserloh's trauma shake, Pittman's jump, Celeste's Player.cs, coyote time and jump buffer, GMTK, Boss Keys, Red Blob Games, Game Programming Patterns, Fix Your Timestep, WFC, rooms and mazes, Spelunky's generator, Dijkstra maps, idle-game maths, genre templates for platformer, twin-stick, tower defense, roguelike, match-3, runner, racing, rhythm, metroidvania, survivors-like, card battler, idle) | 43 | mostly all rights reserved (talks and articles); WFC and Celeste's code MIT; Game Programming Patterns and the cheat book's prose non-commercial | link; each record's `pattern` field is a summary in our own words |
| [Kenney](https://kenney.nl/assets) 3D packs (characters, platformer, city, nature, castle, racing, space, furniture, food, tower defense, dungeon, pirate, survival ...) | 30 | CC0-1.0 (every pack page; `License.txt` inside the zip) | download (zip); the Animated Characters packs are FBX only |
| [KayKit](https://github.com/KayKit-Game-Assets) (Adventurers and their four heroes, Skeletons, Dungeon Remastered, City Builder, Furniture, Halloween, Medieval Hexagon, Prototype, Restaurant, Space Base ...) | 21 | CC0-1.0 (`LICENSE.txt` in each repo) | download for the GitHub repos pinned to a commit; link for the itch-only packs |
| [Quaternius](https://quaternius.com) packs (Universal Animation Library, Ultimate Animated Character, RPG characters, monsters, animals, nature, sci-fi, dungeon ...) | 21 | CC0-1.0 for copies already published, **but** quaternius.com switched to the "Quaternius Asset License" (no redistribution) on 2026-08-28; the Bestiary pack is already under it | link (downloads go through Google Drive or itch); the GitHub mirror of the free UAL (taken before the change) and a Poly Pizza upload are downloadable |
| [Khronos glTF-Sample-Assets](https://github.com/KhronosGroup/glTF-Sample-Assets) at commit edc7c9e6 (Fox, CesiumMan, RiggedFigure, RiggedSimple, SimpleSkin, Flight Helmet, ToyCar, Lantern ...) | 17 | per model: CC0 or CC-BY-4.0 with the exact credit; DamagedHelmet (has CC-BY-NC parts), Sponza (Cryengine agreement) and BrainStem (Poser EULA) are not | download for CC0 and CC-BY; link for the others |
| [three.js example models](https://github.com/mrdoob/three.js/tree/r170/examples/models/gltf) at r170: RobotExpressive (CC0), Flower (CC0), LeePerrySmith (CC-BY-3.0) | 6 | per model; Soldier, Xbot and Michelle are Mixamo, Nefertiti is non-commercial, Littlest Tokyo gives no CC version | download for the first three; link for the rest |
| [Poly Haven](https://polyhaven.com/license) (models, HDRIs, textures) and [ambientCG](https://ambientcg.com/license) | 10 | CC0-1.0 | download for single Poly Haven items (files from its API); link for the libraries |
| [Poly Pizza](https://poly.pizza), [OpenGameArt](https://opengameart.org), [Mixamo](https://www.mixamo.com) | 6 | per item (CC0 or CC-BY on Poly Pizza and OpenGameArt; Mixamo is Adobe's terms) | download for three single CC0 items; link for the libraries and Mixamo |

**Thumbnails ship only when their licence is known:** the pack's own Preview.png from inside a CC0 zip (Kenney, 30),
a CC0 repo's sample image (KayKit and others, 19), a Khronos model's screenshot under the model's licence (14), or a
picture raea rendered from a CC0 or CC-BY model (4, `thumb.py`). Preview images from sites that state no licence
(quaternius.com, Poly Haven's renders, itch covers) are kept as `preview_url` links and are never copied.

## The record format

One JSON object per line. Every record has:

| Field | Meaning |
|---|---|
| `id` | short kebab id, stable (the key for `games-show` and `games-fetch`) |
| `kind` | `asset`, `animation`, `rig`, `example`, `game`, `pattern`, or `plugin` (a Bevy crate, useful when extending the SDK) |
| `title`, `description` | written for search: what it is, its style, what it's for |
| `tags` | words for search |
| `licence`, `attribution` | SPDX-style licence; the exact credit for CC-BY, the author otherwise |
| `ship` | `download` (may be fetched) or `link` (reference only), with `link_reason` |
| `source` | the page the record was made from |

and, where they apply:

| Field | Meaning |
|---|---|
| `files` | `[{url, path, bytes, sha256?}]`, what `games-fetch` downloads (https only); `download_bytes` is their sum |
| `main` | the model file inside the record's folder; packs instead list `contents`, `model_dir` and `examples` |
| `format`, `polycount`, `rig`, `animations`, `morphs`, `textures`, `scale_hint` | for assets: the clip names are the ones to pass to `<Model animation>` |
| `raea` | whether it was checked to render in raea (`renders, skinned and animated (checked ...)`), or why it won't (`fails: JPEG textures ...`, `not usable yet: no environment maps`) |
| `thumbnail`, `thumb_licence` | a shipped picture and its licence; `preview_url` for one that is only linked |
| `tsx` | a raea TSX equivalent (`tsx/NAME.tsx`); `games-show` inlines it as `tsx_source` |
| `topic` | the raea-bevy craft file that covers it (`bevy/lights`, `bevy/games`, `bevy/limits` = not in raea yet, `rows` = extending the SDK) |
| `pattern`, `patterns` | for design patterns and games: the rules and numbers to implement, and the pattern names a game demonstrates |
| `repo`, `bevy_version`, `stars`, `licence_note`, `licence_verified` | for games and crates |

Counts on 2026-10-18: 729 records. By kind: 73 asset, 35 rig, 4 animation, 43 pattern, 116 game, 353 example and
105 plugin. 74 records are downloadable.

TSX equivalents (each renders; `breakout`, `platformer-feel` and `screenshake` attach to design patterns too):
`shapes` (3D shapes), `lighting` (lighting, spotlight), `pbr`, `fog-bloom` (fog, bloom 3D), `camera-orbit` (camera
orbit, smooth follow), `animated-character` (animated mesh, robot-expressive), `breakout` (Breakout, juiced as in
"Juice it or lose it"), `platformer-feel` (coyote time, jump buffer, variable jump height, Pittman's jump),
`screenshake` (trauma-squared shake with smooth noise).

## The index

`raea-meaning games-index` embeds every record with raea-embed's SigLIP 2 (base p16-256, ONNX Runtime, no Python):

- **text**: the mean of two phrases, `"{title}. {description}"` and `"{kind} {title}: {tags}"` (SigLIP's text side
  reads 64 tokens);
- **image**: the thumbnail, when the record has one (67 of them).

The vectors are stored as int8, one f32 scale per vector, in `index.bin` (`RGL2`: n, dim, then n text vectors, then
m image vectors each tagged with its record index): 615 KB for 729 records. Building it takes about 15 s.
`index.json` records the model id, the counts and an FNV hash of `catalogue.jsonl`. A query against a changed
catalogue refuses with "run games-index", so the index can't go stale silently. No new model was added. The HDC
sign-bit projections in `ai/meaning/src/k.rs` weren't needed: int8 keeps the cosines (0.999 to the float vectors, tested)
and at this size a flat scan is instant.

## The query

```
raea-meaning games-find "low-poly knight with sword animations" -k 8 [--kind rig,asset] [--download] [--why]
raea-meaning games-show ID                      # the whole record, with the TSX source inline
raea-meaning games-fetch ID [--file REL] [--into DIR]
```

A query is scored three ways and the three are added as z-scores over the catalogue:

- `1.0 x` its SigLIP text vector against each record's text vector;
- `0.6 x` the same vector against each thumbnail. SigLIP was trained text to image, so pictures answer looks
  ("low-poly", "cartoon"); a record without a thumbnail stands at its text z-score here;
- `0.5 x` BM25 over the title (x3), the tags (x2), the description and the pattern text. This catches names such as
  "breakout" or "coyote time".

A query takes about 0.65 s, most of it loading the text model, which holds about 3 GB resident while it runs.

Example, `games-find "low-poly knight with sword animations" -k 5`:

| score | id | kind | licence | ship |
|---|---|---|---|---|
| 9.03 | oga-low-poly-hero-sword-shield | rig | CC0-1.0 | download |
| 6.60 | oga-lowpoly-dummy | rig | CC0-1.0 | download |
| 6.49 | kaykit-adventurers-knight | rig | CC0-1.0 | download (checked in raea) |
| 6.35 | oga-quaternius-animated-human | rig | CC0-1.0 | download |
| 6.10 | kaykit-adventurers | rig | CC0-1.0 | download |

Other spot checks (top hit first): "how to make a jump feel good in a platformer" gives variable-jump-height,
coyote-time-jump-buffer, celeste-feel-gmtk and building-a-better-jump. "screen shake when something explodes" gives
Bevy's 2d screen_shake example, then art-of-screenshake. "procedural dungeon generation" gives rooms-and-mazes.
"breakout" gives Bevy's Breakout (with its TSX), then juice-it-or-lose-it. "multiplayer rollback netcode" gives
extreme_bevy, then Jumpy. "skeleton enemy undead" gives KayKit's Skeleton Warrior and Skeletons. "furniture for a
house interior" gives KayKit Furniture Bits and Kenney Furniture Kit.

**Fetching.** `games-fetch` runs only for `ship: download` records whose licence is on the list. It downloads each
recorded https URL with curl and checks the sha256 where one is recorded. It unpacks zips, leaving a marker so an
unpacked pack isn't fetched again, and writes `ATTRIBUTION.txt` beside the files in `~/.raea/assets/games/ID/`.
`--into DIR` copies the model (`main`, or `--file REL` picked from a pack) into a space, together with the files a
glTF names: a .gltf's .bin and textures, and also a .glb's external pictures, since Kenney's point at
`Textures/colormap.png`.

**For void's agent.** The agent's shell is sandboxed to the space, with no network and no raea-meaning. So
`apps/void/src/brain.rs` gains one tool, `game_library`. With `query` (and optional `kind`) it runs `games-find -k 8`.
With `fetch` (and `file` for a pack) it runs `games-fetch --into <object>/media/models/ID` and answers with the
`staticFile("models/ID/FILE")` path. The craft list gains `bevy/library`. Nothing in void's UI changed.

## Packaging and sizes

| Part | Size | How it ships |
|---|---|---|
| catalogue + index + index.json + thumbnails + TSX | 1.8 MB (1.04 MB as `raea-games-lib-<fnv>.tar.gz`) | **bundled**: in git, and in the app at `~/.raea/library/games` (`pack.sh lib OUT`; raea-meaning looks there when it isn't run from the repo, or at `$RAEA_GAMES_LIB`) |
| SigLIP 2 text and vision models | already shipped for raea-embed | reused, nothing new |
| starter asset pack: the 12 rigged characters and animation sets under 8 MB (robot, the four KayKit heroes, the Skeleton Warrior, Fox, the RiggedFigure and RiggedSimple samples, SimpleSkin, the Poly Pizza adventurer, the UAL mirror) | 26 MB | **optional release artifact** (`pack.sh assets OUT --starter`), unpacked into `~/.raea/assets/games/` |
| every downloadable record | 357 MB in 74 records (the biggest: Flight Helmet 48 MB, A Beautiful Game 43 MB, SciFi Helmet 30 MB) | **fetched on demand** per record by `games-fetch`, or as a full release artifact (`pack.sh assets OUT`) |
| the fixture's RobotExpressive.glb | 464 KB | committed (CC0), so `check.sh` and `skills.sh` run offline |

Budget: the bundled part stays under 3 MB. A record costs about 0.7 KB of catalogue and 0.8 KB of vectors; a
thumbnail costs about 11 KB and its vector 0.8 KB. Model files are never bundled into the app or committed, apart from
the fixture above. Each asset pack carries a `MANIFEST.json` (id, licence, attribution, source, files) and every
record's `ATTRIBUTION.txt`.

Downloads made while building this: 19 MB in `~/.raea/assets/games/` (six records). Temporary files were deleted.

## End to end

The agent searches, finds a CC0 rigged and animated glTF character, fetches it, and a fixture uses it through
`Model`:

1. `raea-meaning games-find "rigged animated character glTF with walk and run clips" -k 5` puts `robot-expressive`
   first (CC0, Tomás Laulhé / Quaternius; 43 joints, 14 clips, 3 face morphs, 464 KB).
2. `raea-meaning games-fetch robot-expressive --into sdk/motion/bench/bevy/character/media` fetches it from three.js at
   r170, checks its sha256 and copies `RobotExpressive.glb`.
3. `sdk/motion/bench/bevy/character/edit.tsx` (a copy of `tsx/animated-character.tsx`) walks one robot across the floor
   (`animation="Walking"`, its position from the frame) and has a second one wave from a Sequence.
4. `sdk/motion/bench/bevy/check.sh sdk/motion/bench/bevy/character/edit.tsx` renders frames 0, 45 and 89 with
   `raea-motion render` and takes the same frames from a baked play (`play --baked --offscreen --3d --snapshot`):
   **PASS, SSIM 1.000000 on all three**. The lights, materials, model and planes fixtures still pass at 1.000000
   after the engine changes below, and `skills.sh` renders all 10 craft examples, including `bevy/library`'s.

Along the way this gave raea its first skinned fixture (bevy-coverage.md listed skinning as untested). Two engine
fixes in `engine/rdb/src/engine/scene3d.rs` came out of it:

- **Skin weights are normalised on load.** RobotExpressive's hand weights sum to as much as 1.755. glTF requires 1,
  and three.js's GLTFLoader quietly normalises them, so the file looks right there. Bevy draws them as written, and
  the hands stretched into spikes, even in the bind pose. `normalise_joint_weights` rescales only the vertices whose
  sums are off, once, before the scene spawns.
- **A model whose texture fails now fails at once, with the reason.** A JPEG texture (the engine's Bevy build has no
  `jpeg` feature) used to leave the model loading until the 60 s timeout. Now the render refuses in 0.4 s and names
  the file. Records with JPEG textures carry `raea: "fails: JPEG textures ..."`; this affects the Poly Haven models
  and CesiumMan.

Also checked to render, skinned and animated: Khronos Fox and RiggedFigure, the Poly Pizza Quaternius adventurer,
the KayKit Knight (clip `1H_Melee_Attack_Slice_Diagonal`), and a Kenney Mini Character (`walk`). The Kenney
character's long T-pose arms match Kenney's own preview of it.

## Maintaining it

- Add or fix curated records in `sources/*.jsonl`, then `python3 -I sdk/motion/library/games/build.py` and
  `raea-meaning games-index`. `build.py --refresh` re-reads bevy-assets and the GitHub licences, which needs `gh`.
- A new downloadable model: give it `files` (https URLs pinned to a tag or commit where possible), `main`, and a
  licence on the list. Then `python3 -I sdk/motion/library/games/thumb.py ID [--clip NAME]` renders its thumbnail and
  says whether raea can draw it; record the result as `raea`.
- Re-check Quaternius before mirroring anything new from it: its site licence no longer allows redistribution.
- Not there yet: JPEG textures (add Bevy's `jpeg` feature to rdb), environment maps for the HDRIs, clip blending.
  The two Kenney Animated Characters packs are FBX only and need a conversion step. Asset thumbnails cover 67 of 112
  asset, rig and animation records; the rest are text-only in the index.
