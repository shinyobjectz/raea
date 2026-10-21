// The playtest pack the new-stack evals run on any game an agent built (sdk/react/docs/playtest.md): nothing here knows
// the game, so it imports nothing from it. Rules read the bots' report and trace (finishes, softlocks, an end, every
// machine's state and transition coverage, softlocks and unreachable states); clef-omni answers only what needs eyes,
// each question on a clean view of the bots' furthest or winning run. Copied beside the agent's edit as
// edit.playtest.ts by apps/void/evals/tools/playtest_game.sh, which runs run.ts, views.ts and judge.ts with it.
import { choose, clean, grade, noSoftlock, noUnreachable, playtest, rule, yes, LEVELS } from "@raea/motion/playtest";

// the run as a clip a person would watch, a sheet for taste, the furthest stretch, and the finish when there is one
const sessionRun = clean("session.run", { when: "session", as: "clip", stride: 2 });
const sessionLook = clean("session.look", { when: "session", as: "sheet", stride: 60 });
const furthest = clean("furthest", { when: (m) => m.name === "furthest", window: [-90, 0], as: "sheet", stride: 15, limit: 1 });
const finish = clean("finish", { when: (m) => m.name === "finish", window: [-90, 20], as: "clip", stride: 2, limit: 1 });

export default playtest({
  pack: "game-generic",
  version: 1,
  about: "Any game an agent built, from a bot's playtest. Rules read the report and trace with no model call; clef answers taste and legibility questions on clean views. The model checks are advisory: no judged games calibrate them yet.",
  fps: 60,
  views: [sessionRun, sessionLook, furthest, finish],
  checks: [
    rule("play.finishes", "finished", { scope: "session", status: "warn",
      fix: "A bot couldn't reach the goal: report.json's ceiling says where play stops. Make the obstacle there passable." }),
    rule("play.no_softlock", "softlocks", { scope: "session", status: "warn",
      fix: "Some reachable state can't get back to the frontier (report.json softlocks): give the player a way out, or end the game there." }),
    rule("play.ends", "has_outcome", { scope: "session",
      fix: "The game never says won or lost: give it an end the player can see (playtest.outcome)." }),
    rule("machine.states", "state_coverage", { scope: "session", status: "warn",
      fix: "Some machine states never happened in the bots' play (report.json machines.*.stateCoverage.missing)." }),
    rule("machine.transitions", "transition_coverage", { scope: "session",
      fix: "Some machine transitions never fired (machines.*.transitionCoverage.missing): their guards may never hold." }),
    noSoftlock("machine.no_softlock", { scope: "session", status: "warn",
      fix: "A machine state is entered and never left while the input keeps changing (machines.*.softlocks)." }),
    noUnreachable("machine.reachable", { scope: "session", status: "warn",
      fix: "A machine has states no transition reaches (machines.*.unreachable)." }),
    yes("session.goal_clear", "This is a bot's play of a small video game. From what is on screen, is it clear what the player is trying to do (the goal), and what the player controls?", {
      scope: "session", view: sessionRun, taste: true, fault: "no", context: ["session"],
      fix: "Make the goal and the player's character read at once: a visible goal, a counter for what to collect, a character that stands out." }),
    yes("session.feedback", "When something happens in this play (a jump, a pickup, a hit, a win or a loss), does the game answer it visibly on screen within a moment (a flash, a pop, a number, a change of pose)?", {
      scope: "session", view: sessionRun, taste: true, fault: "no", context: ["session"],
      fix: "Give each event an answer the player can see in a few frames: a flash, a squash, a number that pops." }),
    choose("session.kind", "Which best describes what this play shows?", ["working_game", "static_or_broken", "primitive_toy"], {
      describe: {
        working_game: "a working game: a character the input moves, a world with things to do, a goal",
        static_or_broken: "a broken or static scene: nothing moves, black or empty frames, or errors on screen",
        primitive_toy: "a toy: boxes or shapes moving with no readable world or goal",
      },
      scope: "session", view: sessionLook, taste: true, fault: ["static_or_broken"], context: ["session"],
      fix: "The game doesn't read as a working game: check the render for black frames, a camera pointing away, or a state that never moves." }),
    grade("session.looks", "These frames are taken across a play of a small game. How polished and coherent does it look as a game someone would want to play?", LEVELS, {
      scope: "session", view: sessionLook, taste: true, below: "acceptable", context: ["session"],
      fix: "Raise the look: a consistent palette, readable characters against the background, feedback on events." }),
  ],
  decisions: [],
});
