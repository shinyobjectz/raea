// the remodel's plan checked against a kitchen's clearances: raea-motion run kitchen/remodel-check.ts
import { print } from "@raea/motion/native";
import { decorFindings, findings } from "./remodel-plan";
import { OPENINGS } from "./remodel";

const f = [...findings(), ...decorFindings(OPENINGS)];
for (const x of f) print(`${x.level.padEnd(8)} ${x.rule.padEnd(14)} ${x.says}`);
print(`${f.filter((x) => x.level === "error").length} errors, ${f.filter((x) => x.level === "warning").length} warnings`);
