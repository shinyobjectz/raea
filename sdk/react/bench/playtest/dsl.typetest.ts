// The DSL's types against clef-omni's limits: every line marked @ts-expect-error must fail to compile (tsc reports an
// unused directive if it doesn't). Run: sdk/react/bench/playtest/typecheck.sh
import { choose, clean, debug, decide, grade, rate, rule, yes } from "../../../motion/src/playtest";

const look = clean("look", { when: "session", as: "sheet", stride: 60 });
const boxes = debug("boxes", ["hitboxes"], { when: (m) => m.name === "died", window: [-30, 10], as: "clip", stride: 4 });

// valid: one of each
yes("ok.yes", "Is the death readable?", { scope: "moment", view: boxes, fault: "no", fix: "-" });
choose("ok.choose", "What killed the player?", ["the gap", "an enemy", "a timer", "unclear"], { scope: "moment", view: boxes, fault: ["unclear"], fix: "-" });
grade("ok.grade", "How polished?", ["poor", "weak", "acceptable", "strong", "excellent"], { scope: "session", view: look, taste: true, below: "acceptable", fix: "-" });
rate("ok.rate", "How clear is the goal?", ["hidden", "guessable", "obvious"], { scope: "session", view: look, below: "guessable", fix: "-" });
decide("ok.move", "Which move?", { run: ["ArrowRight"], jump: ["ArrowUp"] }, { sees: "frame", hold: 30 });
rule("ok.rule", "finished", { scope: "session", status: "warn", fix: "-" });

// @ts-expect-error a question id clef refuses (a space)
yes("has space", "?", { scope: "moment", view: boxes, fault: "no", fix: "-" });
// @ts-expect-error an empty id
yes("", "?", { scope: "moment", view: boxes, fault: "no", fix: "-" });
// @ts-expect-error a choice with one option (clef needs 2 to 255)
choose("one", "?", ["only"], { scope: "moment", view: boxes, fault: ["only"], fix: "-" });
// @ts-expect-error a fault option that isn't one of the options
choose("bad.fault", "?", ["a", "b"], { scope: "moment", view: boxes, fault: ["c"], fix: "-" });
// @ts-expect-error a score with one level (clef needs 2 to 10)
rate("one.level", "?", ["only"], { scope: "session", view: look, below: "only", fix: "-" });
// @ts-expect-error a score with 11 levels
rate("eleven", "?", ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"], { scope: "session", view: look, below: "2", fix: "-" });
// @ts-expect-error `below` that isn't one of the levels
grade("bad.below", "?", ["low", "high"], { scope: "session", view: look, below: "medium", fix: "-" });
// @ts-expect-error a taste question on a debug view (overlays never reach a taste question)
grade("taste.debug", "How polished?", ["low", "high"], { scope: "moment", view: boxes, taste: true, below: "high", fix: "-" });
// @ts-expect-error a model check that warns without calibration
yes("warn.uncalibrated", "?", { scope: "moment", view: boxes, fault: "no", status: "warn", fix: "-" });
// @ts-expect-error a yes question's fault is "yes" or "no"
yes("bad.yes", "?", { scope: "moment", view: boxes, fault: "maybe", fix: "-" });
// @ts-expect-error a scope void doesn't know
yes("bad.scope", "?", { scope: "level", view: boxes, fault: "no", fix: "-" });
// @ts-expect-error a debug view with no overlays named
debug("bare", [], { when: "session", as: "stills" });
// @ts-expect-error a media kind clef has no input for
clean("pdf", { when: "session", as: "pdf" });

// state machine rules: a state the machine doesn't have is a type error
import { machine, type Snapshot } from "../../../motion/src/machine";
import { coversStates, noSoftlock, reaches } from "../../../motion/src/playtest";
const door = machine<{ sm: Snapshot }, null>()({ id: "door", initial: "shut", states: { shut: { after: { 60: "open" } }, open: { after: { 60: "shut" } } } });
reaches("ok.reaches", door, "open", { scope: "session", fix: "-" });
coversStates("ok.covers", door, { scope: "session", min: 0.5, fix: "-" });
noSoftlock("ok.softlock", { scope: "session", machine: door, loops: true, fix: "-" });
// @ts-expect-error a state the machine doesn't have
reaches("bad.state", door, "ajar", { scope: "session", fix: "-" });
