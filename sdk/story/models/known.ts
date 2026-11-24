// The chained extractor's helper (story_models.py chain): one JSON request a line on stdin, one JSON answer a line on
// stdout, so the story lines are written by raea/story alone.
//   {"prompt": {"answers": [step answers so far], "script": "..."}}  -> {"prompt": "KNOWN ... SCRIPT ..."}
//   {"merge": [step answers]}                                        -> {"story": {...}}
import { knownPrompt, mergeSteps } from "./lib.ts";

const dec = new TextDecoder();
let buf = "";
for await (const chunk of Bun.stdin.stream()) {
  buf += dec.decode(chunk);
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i); buf = buf.slice(i + 1);
    if (!line.trim()) continue;
    let out: unknown;
    try {
      const q = JSON.parse(line);
      if (q.prompt) { const m = mergeSteps(q.prompt.answers); out = { prompt: knownPrompt({ values: m.values, facts: m.facts, agents: m.agents, world: m.world }, q.prompt.script) }; }
      else out = { story: mergeSteps(q.merge) };
    } catch (e) { out = { error: String(e) }; }
    process.stdout.write(JSON.stringify(out) + "\n");
  }
}
