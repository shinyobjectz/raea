// Compiles an EDIT.playtest.ts (sdk/motion/src/playtest.ts DSL) into the check-pack JSON void's checks.rs reads.
//   bun sdk/react/bench/playtest/compile.ts EDIT.playtest.ts [-o PACK.json]
import fs from "node:fs";
import { loadModule } from "./load";
import { compile, type PlaytestSpec } from "../../../motion/src/playtest";

const file = process.argv[2];
const o = process.argv.indexOf("-o");
const spec: PlaytestSpec = (await loadModule(file)).default;
const json = JSON.stringify(compile(spec), null, 1) + "\n";
if (o > 0) fs.writeFileSync(process.argv[o + 1], json);
else process.stdout.write(json);
