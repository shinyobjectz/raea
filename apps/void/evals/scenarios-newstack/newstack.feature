# The new stack, end to end (2026-10-18): state machines (sdk/motion/src/machine.ts), the game library (the game_library
# tool), Manifold's full SDK (sdk/motion/src/solid.ts, CrossSection), the playtest export (sdk/motion/src/games.ts Playtest)
# and the raea-bevy craft. Run by hand (`void eval evals/scenarios-newstack --run DATE-newstack-N`), not nightly.
# Beside what the runner grades, each game is playtested afterwards (sdk/react/bench/playtest: bots, machine coverage,
# softlocks, finishable) and every result's render is held against its play (games/replay.sh, timelines/check.sh):
# void/evals/results/2026-10-18-new-stack.md.
#
# Physics through avian3d isn't asked for: <BevyComponent> needs the plugin build (raea-bevy-sdk motion), and the
# raea-motion void renders with has no plugins compiled in (it refuses the edit: "this build has none compiled in").
# Tone.js (@raea/motion/tone) hadn't landed when these were written, so sound is <Audio> with sao().

@medium:game
Feature: Games and 3D on the new stack

  @level:hard
  Scenario: A 3D arena with a library character and state machines
    Given an empty space
    When the person says "make a small 3D arena game at 1920x1080. The player is a rigged, animated CC0 character you find and fetch from the game library (not a box), walking around a walled square arena with the arrow keys and jumping with space. Give the player a state machine (idle, run, jump, hurt) that also picks the model's animation clip. Three enemies patrol the arena, each on its own state machine (patrol, then chase when the player comes near), and touching one hurts the player. Collect 5 gems to win; 3 hits and it's game over. Sound effects for the jump, a gem and a hit. Add a scripted input track so the render plays through and picks up gems, and export the playtest object (initial, step, actions, progress, outcome, machines) so a bot can check it can be won."
    Then expect edit.width = 1920 and edit.height = 1080
    And the judge sees "a 3D arena seen from above and behind, a rigged animated character (not a primitive box) moving around it, enemies moving, gems, a score or gem counter on screen, and the character's pose changing between standing, running and jumping"
    And it costs under $2.50

  @level:hard
  Scenario: A 2D platformer that can be won, on state machines
    Given an empty space
    When the person says "make a 2D side-scrolling platformer at 1920x1080 that can actually be won: run with the arrow keys, jump with space, gaps and raised platforms, two kinds of enemies (a walker that patrols a platform and a hopper that jumps in place), coins, and a flag at the end of the level that wins. The player, each enemy and the game's flow (playing, won, game over) are state machines. The level has to be beatable: export the playtest object (initial, step, actions, progress, outcome, machines) so a bot can finish it, and add a scripted input track so the render plays a run. Jump, coin, stomp and win sounds."
    Then expect edit.width = 1920 and edit.height = 1080
    And the judge sees "a side-scrolling platformer: a player character running and jumping across platforms and gaps, enemies, coins, a camera that follows the player, and a goal flag"
    And it costs under $2.50

@medium:scene3d
Feature: A product shot on Manifold

  @level:hard
  Scenario: A perfume product shot from Manifold solids and a library model
    Given an empty space
    When the person says "make a 6 second 1080x1350 product shot for a made-up perfume called NOCTA. Model the bottle yourself with Manifold: a CrossSection outline (a faceted or rounded shape) extruded into the glass, hollowed with a boolean, a cap made from solids joined and cut, and the name NOCTA cut into the front. Put it on a plinth with one 3D model from the game library beside it as a prop (a plant, a rock or a flower), give it a slow camera move with good light, and end on the name and 'Eau de Parfum' as a title."
    Then expect edit.width = 1080 and edit.height = 1350
    And the judge sees "a modelled perfume bottle with a cap and the word NOCTA on it, standing on a plinth beside a small prop, lit like a product shot, the camera moving slowly, ending on the name NOCTA and 'Eau de Parfum'"
    And it costs under $1.50

@medium:ugc
Feature: An ad edit on synthetic footage (the regression check)

  @level:medium @design
  Scenario: A cold brew ad from a voiceover and b-roll
    Given the space "synth"
    When the person says "media/ has a voiceover and three b-roll shots of Plumquiver, a cold brew coffee. Plumquiver is a fictional brand I made up for this test: it has no website, logo or brand kit, so don't look it up or run raea brand; invent its look yourself. Make a 20 second vertical ad from them: the voiceover leads, the b-roll under it, captions on the words, and an end card with the offer (first case half off at plumquiver.example)."
    Then expect edit.width = 1080 and edit.height = 1920
    And the judge sees "a vertical ad for Plumquiver cold brew: a cold brew can on screen, captions in time with the voice, and an end card with the half-off offer and plumquiver.example"
    And it costs under $1.50
