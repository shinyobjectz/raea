// The sandbox probe's command set (sandbox/wsx's suite: ls, wc, sort, head, grep, jq, a write and read back, the two
// escapes, find, sed) and the rest of the shell's tools, run through @raea/shell on a fixed project; every command's
// stdout and exit must equal probe-golden.json, recorded on the Mac. The same file runs on the iPhone
// (react/ios/test.sh), so the two machines give identical outputs.
//   record: RAEA_PROBE_RECORD=1 raea-motion test shell/conformance/probe.test.ts
import { describe, expect, test } from "bun:test";
import { hasNative, native } from "@raea/motion/native";
import { Shell } from "../src";
import golden from "./probe-golden.json";

const engine = hasNative();

const notes = Array.from({ length: 200 }, (_, k) => {
  const i = k + 1;
  return `take ${String((i * 37) % 200).padStart(3, "0")} ${i % 7 === 0 ? "mesh sign" : "talking head"}\n`;
}).join("");

const COMMANDS = [
  "ls -R",
  "wc -l notes.txt",
  "sort -r notes.txt | head -3",
  "head -n 3 notes.txt",
  "grep -rn mesh . | wc -l",
  "grep -rn mesh . | head -2",
  "jq '.clips | map(.to - .from) | add' clips.json",
  "cp notes.txt edits/notes-copy.txt && wc -c edits/notes-copy.txt",
  "cat /etc/hosts",
  "cat ../../../etc/hosts",
  "find . -name '*.rrl'",
  "sed 's/talking head/TALKING/g' notes.txt | grep -c TALKING",
  "awk '{print $2}' notes.txt | sort -n | uniq | wc -l",
  "cut -d' ' -f3- notes.txt | sort | uniq -c",
  "tr a-z A-Z < edits/sign.rrl",
  "sha256sum notes.txt",
  "gzip -c notes.txt > n.gz && gunzip -c n.gz | sha256sum",
  "base64 clips.json | head -1",
  "seq 1 10 | xargs -n 3 echo",
  "printf 'a\\nb\\n' > x && printf 'a\\nc\\n' > y && diff x y",
  "sqlite3 db.sqlite \"CREATE TABLE t(n INTEGER); INSERT INTO t VALUES (1),(2),(3); SELECT sum(n) * 2 FROM t;\"",
  "js-exec -c 'console.log([3, 1, 2].sort().join(\"-\"), (2 ** 52).toString(16))'",
  "solid 'minus(box(2, 2, 2), cylinder(0.5, 3))' empty triangles",
  "ecs w.json spawn ball x=3 && ecs w.json spawn ball x=9 && ecs w.json query ball 'x > 5'",
  "jaq '.clips[1].id' clips.json",
  "for f in notes.txt clips.json; do printf '%s %s\\n' $f $(wc -c < $f); done",
  "mkdir -p deep/a/b && touch deep/a/b/c && find deep -type f",
  "rm -r deep && ls deep",
];

describe.skipIf(!engine)("the probe command set", () => {
  test("every command's output and exit as recorded", async () => {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
    const sh = new Shell({ root: `${tmp}/raea-shell-probe-${Date.now()}` });
    await sh.fs.mkdir("/edits", { recursive: true });
    await sh.fs.writeFile("/notes.txt", notes);
    await sh.fs.writeFile("/edits/sign.rrl", 'mesh sign {shape: "minus(rounded_box(2.4, 1.1, 0.25, 0.08), text3d(\\"SALE\\", 0.62, 0.6))"}\n');
    await sh.fs.writeFile("/clips.json", '{"clips":[{"id":"a","from":0,"to":77},{"id":"b","from":77,"to":793},{"id":"c","from":793,"to":922}]}');
    const got: Record<string, { exit: number; stdout: string }> = {};
    for (const c of COMMANDS) {
      const r = await sh.exec(c);
      got[c] = { exit: r.exitCode, stdout: r.stdout };
    }
    if (native("host.env", { name: "RAEA_PROBE_RECORD" }) === "1") {
      const root = native("host.env", { name: "RAEA_ROOT" }) as string;
      native("media.write", { path: `${root}/shell/conformance/probe-golden.json`, data: btoa(unescape(encodeURIComponent(JSON.stringify(got, null, 1) + "\n"))), truncate: true });
      return;
    }
    for (const c of COMMANDS) expect({ cmd: c, ...got[c] }).toEqual({ cmd: c, ...(golden as Record<string, { exit: number; stdout: string }>)[c] });
  }, 120000);
});
