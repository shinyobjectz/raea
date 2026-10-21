// LEDGE's playtest (sdk/react/docs/playtest.md): what clef-omni is shown of a bot's run (views), what it's asked (checks
// in clef's own question types) and how a model-driven bot moves (a decision). Views are clean for taste and quality,
// debug (edit.tsx's <Overlay>s drawn) for diagnosis. `bun sdk/react/bench/playtest/compile.ts` writes the check pack.
import { clean, coversStates, coversTransitions, debug, decide, grade, noSoftlock, noUnreachable, playtest, reaches, rule, yes, LEVELS } from "@raea/motion/playtest";
import { flow, player } from "./edit";

// ---- views ----------------------------------------------------------------------------------------------------------
// a life lost: the second before and a little after, clean (is it readable?) and with hitboxes (why did it happen?)
const death = clean("death", { when: (m) => m.name === "died", window: [-90, 30], as: "clip", stride: 4, limit: 1 });
const deathDebug = debug("death.debug", ["hitboxes", "state"], { when: (m) => m.name === "died", window: [-90, 30], as: "clip", stride: 4, limit: 1 });
const deathSheet = debug("death.sheet", ["hitboxes"], { when: (m) => m.name === "died", window: [-60, 0], as: "sheet", stride: 10, limit: 1 });
// where the explorer kept coming back without getting further
const stuck = debug("stuck", ["hitboxes", "state"], { when: (m) => m.name === "stuck", window: [0, 0], as: "stills", limit: 1 });
// the end of the furthest run, and the finish when there is one
const furthest = clean("furthest", { when: (m) => m.name === "furthest", window: [-120, 0], as: "sheet", stride: 15, limit: 1 });
const finish = clean("finish", { when: (m) => m.name === "finish", window: [-120, 30], as: "clip", stride: 4, limit: 1 });
// the moment the player is hurt, from the run's machine transitions (moments named "<entity>.<state entered>")
const hurt = debug("hurt", ["hitboxes", "state"], { when: (m) => m.name === "player.hurt", window: [-60, 20], as: "clip", stride: 4, limit: 1 });
// the run as text (clef reads it with no picture), as video (the replay a person watches), and as a sheet for taste
const sessionText = clean("session.text", { when: "session", as: "text", stride: 30 });
const sessionRun = clean("session.run", { when: "session", as: "clip", stride: 4 });
const sessionLook = clean("session.look", { when: "session", as: "sheet", stride: 120 });

export default playtest({
  pack: "game",
  version: 1,
  about: "A game, from a bot's playtest (sdk/react/docs/playtest.md). Rules read the run's report and trace (no model call). clef answers only what needs eyes, each question on the view it names: clean views for taste and quality, debug views (overlays drawn) for diagnosis. Every model check is advisory until calibration on judged games shows it separates good from bad.",
  fps: 120,
  views: [death, deathDebug, deathSheet, stuck, furthest, finish, hurt, sessionText, sessionRun, sessionLook],
  checks: [
    rule("play.finishes", "finished", { scope: "session", status: "warn",
      fix: "A bot couldn't reach the goal: report.json's ceiling says where play stops. Make the obstacle there passable (a jump that clears it, a platform, a shorter gap)." }),
    rule("play.no_softlock", "softlocks", { scope: "session", status: "warn",
      fix: "Some reachable state can't get back to the frontier (report.json softlocks): give the player a way out, or end the game there." }),
    rule("play.ends", "has_outcome", { scope: "session",
      fix: "The game never says won or lost (no finish, lives that wrap): give it an end the player can see." }),
    rule("play.death_loop", "death_loop", { scope: "session", status: "warn",
      fix: "Most deaths in the search happen in one 400-px stretch: the respawn puts the player back in front of the same hazard. Respawn past it, or make it clearable." }),
    // the state machines (sdk/react/docs/state-machines.md): every state and transition exercised, nothing stuck or unreachable
    coversStates("machine.player.states", player, { scope: "session", status: "warn",
      fix: "Some of the player's states never happened in the bots' play (report.json machines.player.stateCoverage.missing): make them reachable in play, or remove them." }),
    coversTransitions("machine.player.transitions", player, { scope: "session", min: 0.9,
      fix: "Some of the player's transitions never fired (machines.player.transitionCoverage.missing): their guards may never hold." }),
    coversTransitions("machine.flow.transitions", flow, { scope: "session",
      fix: "Part of the game's flow never happens (machines.flow.transitionCoverage.missing): a win, a game over or a restart the bots can't reach." }),
    noSoftlock("machine.no_softlock", { scope: "session", status: "warn",
      fix: "A machine state is entered and never left, though the input keeps changing (machines.*.softlocks): give it a way out, or make it final." }),
    noUnreachable("machine.reachable", { scope: "session", status: "warn",
      fix: "A machine has states no transition reaches (machines.*.unreachable): remove them or add a way in." }),
    reaches("machine.flow.won", flow, "won", { scope: "session", status: "warn",
      fix: "The bots never reach the win: see report.json's ceiling for where play stops." }),
    reaches("machine.flow.game_over", flow, "gameOver", { scope: "session",
      fix: "The game can't be lost: lives that never run out leave the player nothing at stake." }),
    yes("moment.death_readable", "In this clip from a side-scrolling game, the player character loses a life. Is it visually clear what caused it (falling into a pit, or touching an enemy)?", {
      scope: "moment", view: death, taste: true, fault: "no", context: ["moment", "observe"],
      fix: "Make the cause of a death readable: a visible pit edge, an enemy that reads as dangerous, a beat of reaction before the respawn." }),
    yes("moment.death_fair", "Collision boxes are drawn: green is the player, magenta outlines are enemies, cyan lines are platform tops. Does the player lose the life without its green box touching a magenta box, while still over solid ground?", {
      scope: "moment", view: deathDebug, fault: "yes", context: ["moment"],
      fix: "The death looks unfair against the hitboxes: tighten the hazard's box or the player's." }),
    grade("session.looks", "These frames are taken across a play of a small 2D game. How polished and coherent does it look as a game someone would want to play?", LEVELS, {
      scope: "session", view: sessionLook, taste: true, below: "acceptable", context: ["session"],
      fix: "Raise the look: a consistent palette, readable characters against the background, feedback on events." }),
  ],
  decisions: [
    decide("move", "You are playing this side-scrolling platformer; the pale rounded block is you. The goal is the far right end of the level. Falling into a gap or touching a red walker costs a life and sends you back. Which move should you make for the next quarter second?", {
      run_right: ["ArrowRight"], jump_right: ["ArrowRight", "ArrowUp"], jump_up: ["ArrowUp"], wait: [], run_left: ["ArrowLeft"], jump_left: ["ArrowLeft", "ArrowUp"],
    }, { sees: "frame", hold: 30 }),
  ],
});
