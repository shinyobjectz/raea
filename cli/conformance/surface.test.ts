// The agent's surface can't drift: what `raea help` lists, what the CLI asks of the host, what void's host answers
// (void/src/host.rs), and every `raea <command>` the agent's context mentions (void's prompts, AGENTS.md) must agree.
// One source per fact: commands live in HELP, so a prompt naming a command HELP doesn't list fails here.
import { describe, expect, test } from "bun:test";
import { readFileSync, readdirSync, statSync } from "fs";
import { join } from "path";

const here = join(import.meta.dir, "..");
const raea = join(here, "..");
const cli = readFileSync(join(here, "src/index.ts"), "utf8");
const host = readFileSync(join(raea, "void/src/host.rs"), "utf8");

const help = /export const HELP = `([\s\S]*?)`;/.exec(cli)![1];
/** the commands HELP lists: `  raea NAME ...` lines */
const listed = new Set([...help.matchAll(/^\s+raea (\w+)/gm)].map((m) => m[1]));
/** the commands the CLI handles: `case "name":` in the handler */
const handled = new Set([...cli.matchAll(/case "(\w+)":/g)].map((m) => m[1]).filter((c) => c !== "--help"));
/** the host ops the CLI asks for: ask("op" or ask(args[0] for render/check */
const asked = new Set([...cli.matchAll(/ask\("(\w+)"/g)].map((m) => m[1]));
if (/ask\(args\[0\]/.test(cli)) for (const c of ["render", "check"]) asked.add(c);
/** the ops void's host answers: `"name" =>` arms in handle() */
const handleFn = /pub async fn handle[\s\S]*?\n}\n/.exec(host)![0];
const answered = new Set([...handleFn.matchAll(/^\s+"(\w+)" =>/gm)].map((m) => m[1]));

function files(dir: string, out: string[] = []): string[] {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) files(p, out);
    else if (f.endsWith(".md")) out.push(p);
  }
  return out;
}

describe("raea's surface", () => {
  test("every command HELP lists is handled, and every handled command is listed", () => {
    expect([...listed].filter((c) => !handled.has(c))).toEqual([]);
    expect([...handled].filter((c) => !listed.has(c))).toEqual([]);
  });

  test("every host op the CLI asks for is answered by void's host, and the host answers nothing the CLI can't ask", () => {
    expect([...asked].filter((o) => !answered.has(o))).toEqual([]);
    expect([...answered].filter((o) => !asked.has(o))).toEqual([]);
  });

  test("every command the CLI hands to the project's record is one @raea/data/project.ts runs", () => {
    const record = readFileSync(join(raea, "data/src/project.ts"), "utf8");
    const runs = new Set([...(/export function run[\s\S]*/.exec(record)![0]).matchAll(/case "(\w+)":/g)].map((m) => m[1]));
    // the cases that fall through to ask("project", ...) in the CLI, and restore
    const block = /((?:\s+case "\w+":\n)+)\s+case "checkout": \{\n\s+const r = ask\("project"/.exec(cli)!;
    const forwarded = [...block[1].matchAll(/case "(\w+)":/g)].map((m) => m[1]).concat(["checkout", "restore"]);
    expect(forwarded.length).toBeGreaterThan(10);
    expect(forwarded.filter((c) => !runs.has(c))).toEqual([]);
  });

  test("every `raea <command>` in the agent's context exists", () => {
    const sources = [...files(join(raea, "void/prompts")), "AGENTS.md (cli NOTES)"];
    const bad: string[] = [];
    for (const src of sources) {
      const text = src.startsWith("AGENTS.md") ? /const NOTES = `([\s\S]*?)`;/.exec(cli)![1] : readFileSync(src, "utf8");
      for (const m of text.matchAll(/`raea (\w+)/g)) if (!listed.has(m[1])) bad.push(`${src.replace(raea + "/", "")}: raea ${m[1]}`);
    }
    expect(bad).toEqual([]);
  });
});
