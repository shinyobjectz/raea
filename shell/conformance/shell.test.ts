// @raea/shell on the engine (raea-motion test): bash over a project folder, kept inside it, every command an action
// row that can be undone and redone, and raea's tools as commands.
import { describe, expect, test } from "bun:test";
import { hasNative, native } from "@raea/motion/native";
import { Shell } from "../src";

const engine = hasNative();
let n = 0;
const fresh = () => {
  const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
  return new Shell({ root: `${tmp}/raea-shell-test-${Date.now()}-${++n}` });
};

describe.skipIf(!engine)("the shell", () => {
  test("bash: pipes, loops, functions, text tools", async () => {
    const sh = fresh();
    await sh.exec("printf 'banana\\napple\\ncherry\\napple\\n' > fruit.txt");
    expect((await sh.exec("sort fruit.txt | uniq -c | sort -rn | head -1")).stdout.trim()).toBe("2 apple");
    expect((await sh.exec("f() { echo \"<$1>\"; }; for x in a b; do f $x; done")).stdout).toBe("<a>\n<b>\n");
    expect((await sh.exec("echo '{\"a\":[1,2,3]}' | jq '.a | add'")).stdout.trim()).toBe("6");
    expect((await sh.exec("seq 4 | awk '{s+=$1} END {print s}'")).stdout.trim()).toBe("10");
  });

  test("files land in the project folder", async () => {
    const sh = fresh();
    await sh.exec("mkdir -p a/b && echo hi > a/b/c.txt");
    expect(native("media.size", { path: `${sh.fs.root}/a/b/c.txt` })).toBe(3);
    expect((await sh.exec("cat /a/b/c.txt")).stdout).toBe("hi\n");
  });

  test("nothing outside the project is reachable", async () => {
    const sh = fresh();
    const up = await sh.exec("cat /../../../../etc/passwd");
    expect(up.exitCode).not.toBe(0);
    expect(up.stdout).toBe("");
    await sh.exec("ln -s ../../../../../../../etc/passwd out");
    const viaLink = await sh.exec("cat out");
    expect(viaLink.exitCode).not.toBe(0);
    expect(viaLink.stdout).toBe("");
    await sh.exec("ln -s /etc/passwd inside");
    expect((await sh.exec("cat inside")).exitCode).not.toBe(0);
  });

  test("the records folder is hidden", async () => {
    const sh = fresh();
    await sh.exec("echo x > a");
    expect((await sh.exec("ls -a /")).stdout).not.toContain(".raea");
    expect((await sh.exec("cat /.raea/actions.jsonl")).exitCode).not.toBe(0);
    // as if it weren't there: rm -f of a missing path succeeds, and the records stay
    await sh.exec("rm -rf /.raea");
    expect(sh.log().length).toBeGreaterThan(1);
    expect((await sh.exec("cat a")).stdout).toBe("x\n");
  });

  test("each command is one action row: verb, arguments, files read and written, state hashes", async () => {
    const sh = fresh();
    const w = await sh.exec("echo hi > a.txt");
    const row = w.row as any;
    expect(row.cmd).toBe("echo hi > a.txt");
    expect(row.verb).toBe("echo");
    expect(row.exit).toBe(0);
    expect(row.written).toHaveLength(1);
    expect(row.written[0].path).toBe("/a.txt");
    expect(row.written[0].before).toBeNull();
    expect(row.written[0].after.kind).toBe("file");
    expect(row.state_after).not.toBe(row.state_before);
    const r = (await sh.exec("cat a.txt")).row as any;
    expect(r.read).toEqual(["/a.txt"]);
    expect(r.written).toEqual([]);
    expect(r.state_after).toBe(r.state_before);
    const bad = (await sh.exec("cat nope")).row as any;
    expect(bad.exit).toBe(1);
    expect(bad.error).toContain("nope");
  });

  test("undo and redo, for a file and a whole folder", async () => {
    const sh = fresh();
    await sh.exec("echo hi > a.txt");
    await sh.exec("echo bye > a.txt");
    expect((sh.undo() as any).kind).toBe("undo");
    expect((await sh.exec("cat a.txt")).stdout).toBe("hi\n");
    sh.redo();
    expect((await sh.exec("cat a.txt")).stdout).toBe("bye\n");
    await sh.exec("mkdir -p d/e && echo 1 > d/e/f && echo 2 > d/g");
    await sh.exec("rm -r d");
    expect((await sh.exec("ls d")).exitCode).not.toBe(0);
    sh.undo();
    expect((await sh.exec("cat d/e/f d/g")).stdout).toBe("1\n2\n");
    sh.undo();
    expect((await sh.exec("ls d")).exitCode).not.toBe(0);
    expect((await sh.exec("cat a.txt")).stdout).toBe("bye\n");
  });

  test("a reopened project keeps its history", async () => {
    const sh = fresh();
    await sh.exec("echo one > n");
    await sh.exec("echo two > n");
    const again = new Shell({ root: sh.fs.root });
    again.undo();
    expect((await again.exec("cat n")).stdout).toBe("one\n");
  });
});

describe.skipIf(!engine)("raea's commands", () => {
  test("solid: Manifold from an expression", async () => {
    const sh = fresh();
    const r = await sh.exec("solid 'minus(box(2, 2, 2), cylinder(0.5, 3))' volume 'slice' 0");
    const [vol, slice] = r.stdout.trim().split("\n");
    expect(Math.abs(Number(vol.split(" ")[1]) - (8 - Math.PI * 0.5))).toBeLessThan(0.02);
    expect(slice).toContain("2 contours");
    expect((await sh.exec("solid 'box(1, 1'")).exitCode).toBe(1);
  });

  test("ecs: a world in a file", async () => {
    const sh = fresh();
    expect((await sh.exec("ecs w.json spawn ball x=1 color=red")).stdout).toBe("1\n");
    await sh.exec("ecs w.json spawn ball x=5 color=blue");
    await sh.exec("ecs w.json spawn wall x=0");
    expect((await sh.exec("ecs w.json query ball 'x > 2'")).stdout).toBe("2\n");
    expect((await sh.exec("ecs w.json count ball")).stdout).toBe("2\n");
    await sh.exec("ecs w.json despawn 1");
    expect((await sh.exec("ecs w.json query ball")).stdout).toBe("2\n");
    expect((await sh.exec("ecs w.json query wall")).stdout).toBe("3\n");
    expect(JSON.parse((await sh.exec("cat w.json")).stdout)).toHaveLength(2);
  });

  test("rdb runs in process, in the shell's folder, with project paths", async () => {
    const sh = fresh();
    await sh.exec("mkdir edits && cd edits");
    const made = await sh.exec("cd /edits && rdb new e.rdb");
    expect(made.exitCode).toBe(0);
    expect(made.stdout).toContain("e.rdb: an empty edit");
    expect(made.stdout).not.toContain(sh.fs.root);
    expect((await sh.exec("ls /edits")).stdout).toBe("e.rdb\n");
    expect((await sh.exec("rdb moves /edits/e.rdb")).exitCode).toBe(0);
    const missing = await sh.exec("rdb moves /nope.rdb");
    expect(missing.exitCode).toBe(1);
    expect(missing.stderr).toContain("/nope.rdb");
    expect(missing.stderr).not.toContain(sh.fs.root);
    // rrl the language is retired (2026-10-17): no rrl command
    expect((await sh.exec("rrl")).exitCode).not.toBe(0);
  });

  test("media: sizes and errors in the shell's paths", async () => {
    const sh = fresh();
    await sh.exec("printf 'abcd' > f");
    expect((await sh.exec("media size f")).stdout).toBe("4\n");
    const bad = await sh.exec("media transcode nope.mp4 out.mp4");
    expect(bad.exitCode).toBe(1);
    expect(bad.stderr).not.toContain(sh.fs.root);
  });
});

describe.skipIf(!engine)("the native extras", () => {
  test("sqlite3: SQLite on a project file, output modes, errors", async () => {
    const sh = fresh();
    await sh.exec(`sqlite3 db.sqlite "CREATE TABLE t(a INTEGER, b TEXT); INSERT INTO t VALUES (1, 'x'), (2, NULL), (3, 'a,b');"`);
    expect((await sh.exec(`sqlite3 db.sqlite "SELECT * FROM t"`)).stdout).toBe("1|x\n2|\n3|a,b\n");
    expect((await sh.exec(`sqlite3 -header -csv db.sqlite "SELECT * FROM t WHERE a > 1"`)).stdout).toBe("a,b\r\n2,\r\n3,\"a,b\"\r\n");
    expect(JSON.parse((await sh.exec(`sqlite3 -json db.sqlite "SELECT a, b FROM t WHERE a = 1"`)).stdout)).toEqual([{ a: 1, b: "x" }]);
    expect((await sh.exec(`echo "SELECT count(*) FROM t;" | sqlite3 db.sqlite`)).stdout).toBe("3\n");
    expect((await sh.exec(`sqlite3 :memory: "SELECT 1 + 1"`)).stdout).toBe("2\n");
    const bad = await sh.exec(`sqlite3 db.sqlite "SELEC 1"`);
    expect(bad.exitCode).toBe(1);
    expect(bad.stderr).toContain("Error");
    // the database file is the project's: its row has it, and undo takes the insert back
    const ins = (await sh.exec(`sqlite3 db.sqlite "INSERT INTO t VALUES (4, 'y')"`)).row as any;
    expect(ins.written.map((w: any) => w.path)).toContain("/db.sqlite");
    sh.undo();
    expect((await sh.exec(`sqlite3 db.sqlite "SELECT count(*) FROM t"`)).stdout).toBe("3\n");
  });

  test("js-exec: TypeScript on raea's V8, imports inside the project, no host calls", async () => {
    const sh = fresh();
    expect((await sh.exec(`js-exec -c 'const n: number = 2 + 3; console.log("n", n)'`)).stdout).toBe("n 5\n");
    await sh.exec(`mkdir lib && echo 'export const sq = (x: number) => x * x;' > lib/m.ts`);
    await sh.exec(`echo 'import { sq } from "./lib/m.ts"; console.log(sq(Number(process.argv[1])));' > main.ts`);
    expect((await sh.exec("js-exec main.ts 7")).stdout).toBe("49\n");
    const out = await sh.exec(`js-exec -c 'import x from "/etc/passwd"; console.log(x)'`);
    expect(out.exitCode).toBe(1);
    const host = await sh.exec(`js-exec -c '(globalThis as any).__raea_call("fs.open", JSON.stringify({ root: "/" }))'`);
    expect(host.exitCode).toBe(1);
    expect(host.stderr).toContain("not allowed");
    expect((await sh.exec(`js-exec -c 'process.exit(3)'`)).exitCode).toBe(3);
    expect((await sh.exec(`js-exec -c 'await new Promise((r) => setTimeout(r, 20)); console.error("late")'`)).stderr).toBe("late\n");
    const spin = await sh.exec(`js-exec --timeout 300 -c 'while (true) {}'`);
    expect(spin.exitCode).toBe(124);
    expect((await sh.exec(`echo hi | js-exec -c 'console.log(process.stdin.text.trim().toUpperCase())'`)).stdout).toBe("HI\n");
  });

  test("python stays off", async () => {
    const sh = fresh();
    const r = await sh.exec("python3 -c 'print(1)'");
    expect(r.exitCode).toBe(127);
  });

  test("curl: only what the host's policy allows, credentials added outside", async () => {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
    const none = new Shell({ root: `${tmp}/raea-shell-net-${Date.now()}` });
    expect((await none.exec("curl -s https://example.com")).exitCode).toBe(127);
    const sh = new Shell({ root: `${tmp}/raea-shell-net2-${Date.now()}`, network: { allow: [{ prefix: "https://example.com/" }] } });
    const r = await sh.exec("curl -s https://example.com/");
    expect(r.exitCode).toBe(0);
    expect(r.stdout).toContain("Example Domain");
    const denied = await sh.exec("curl -sS https://www.iana.org/");
    expect(denied.exitCode).not.toBe(0);
    expect(denied.stderr + denied.stdout).toContain("denied");
    const post = await sh.exec("curl -sS -X POST -d x=1 https://example.com/");
    expect(post.exitCode).not.toBe(0);
  });
});

describe.skipIf(!engine)("the split with wsx", () => {
  test("WASI tools the shell lacks run in the sandbox, in the project", async () => {
    const sh = fresh();
    let tools: string[] = [];
    try {
      tools = native("wsx.tools", {});
    } catch {}
    if (!tools.includes("jaq")) return;
    await sh.exec(`mkdir d && echo '{"a": [1, 2, 3]}' > d/x.json`);
    const r = await sh.exec("jaq '.a | add' /d/x.json");
    expect(r.exitCode).toBe(0);
    expect(r.stdout.trim()).toBe("6");
    // ls and cat stay just-bash's, though rtools has them too
    expect((await sh.exec("ls /d")).stdout).toBe("x.json\n");
  });
});

describe.skipIf(!engine)("tar and html-to-markdown, natively", () => {
  test("tar: create, list, extract, gzip; members that climb out are refused", async () => {
    const sh = fresh();
    await sh.exec("mkdir -p src/a && echo one > src/a/1.txt && echo two > src/2.txt");
    expect((await sh.exec("tar -czf out.tgz src && tar -tzf out.tgz | sort")).stdout).toBe("src/\nsrc/2.txt\nsrc/a/\nsrc/a/1.txt\n");
    const x = await sh.exec("mkdir dest && tar -xzf out.tgz -C dest && cat dest/src/a/1.txt dest/src/2.txt");
    expect(x.stdout).toBe("one\ntwo\n");
    expect((x.row as any).written.map((w: any) => w.path)).toContain("/dest/src/a/1.txt");
    sh.undo();
    expect((await sh.exec("ls dest")).stdout).toBe("");
    // an archive whose member climbs out (made natively, as a hostile archive would be)
    const tmp = native("host.env", { name: "TMPDIR" }) as string;
    const evil = `${sh.fs.root}/evil.tar`;
    const name = "../../escaped.txt";
    const hdr = new Uint8Array(512);
    const put = (s: string, at: number) => { for (let i = 0; i < s.length; i++) hdr[at + i] = s.charCodeAt(i); };
    put(name, 0); put("0000644\0", 100); put("0000000\0", 108); put("0000000\0", 116); put("00000000004\0", 124); put("00000000000\0", 136); put("        ", 148); hdr[156] = 48; put("ustar\0", 257); put("00", 263);
    let sum = 0; for (const b of hdr) sum += b; put(sum.toString(8).padStart(6, "0") + "\0 ", 148);
    const body = new Uint8Array(512); body.set([104, 105, 33, 10]);
    const all = new Uint8Array(512 * 4); all.set(hdr, 0); all.set(body, 512);
    native("media.write", { path: evil, data: btoa(String.fromCharCode(...all)), truncate: true });
    const bad = await sh.exec("tar -xf evil.tar");
    expect(bad.exitCode).not.toBe(0);
    expect(bad.stderr).toContain("leaves the folder");
    expect(() => native("media.size", { path: `${tmp}escaped.txt` })).toThrow();
  });
  test("html-to-markdown from stdin and from a file", async () => {
    const sh = fresh();
    expect((await sh.exec(`echo '<h1>Hi</h1><p>a <b>b</b> <a href="https://x.y">link</a></p><script>bad()</script>' | html-to-markdown`)).stdout).toBe("# Hi\n\na **b** [link](https://x.y)\n");
    await sh.exec(`printf '<ul><li>één</li><li>二</li></ul>' > l.html`);
    expect((await sh.exec("html-to-markdown l.html")).stdout).toBe("*   één\n*   二\n");
  });
});

describe.skipIf(!engine)("one terminal, kept between commands", () => {
  test("cd and export carry over; /dev/null swallows", async () => {
    const sh = fresh();
    expect((await sh.exec("mkdir -p d && cd d && pwd 2>/dev/null")).stdout).toBe("/d\n");
    await sh.exec("export X=5");
    expect((await sh.exec("pwd; echo $X; echo gone > /dev/null; cat /dev/null")).stdout).toBe("/d\n5\n");
    expect((await sh.exec("ls /")).stdout).toBe("d\n");
  });
  test("a deadline stops a command (exit 124) and the shell goes on", async () => {
    const { handle } = await import("../src/serve");
    const sh = fresh();
    const r = await handle(sh, { id: 1, op: "exec", cmd: "sleep 2; echo late", timeout_ms: 200 });
    expect(r.exitCode).toBe(124);
    expect(r.stdout).toBe("");
    expect((await handle(sh, { id: 2, cmd: "echo after" })).stdout).toBe("after\n");
    const big = await handle(sh, { id: 3, cmd: "seq 1 30000" });
    expect(String(big.stdout)).toContain("[cut:");
    expect((await handle(sh, { id: 4, op: "nope" })).error).toContain("no op");
  });
});

describe.skipIf(!engine)("links the host placed", () => {
  test("read through, never written through; the shell can't make one that leads out", async () => {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
    const outside = `${tmp}raea-outside-${Date.now()}`;
    const o = native("fs.open", { root: outside });
    native("fs.write", { id: o.id, path: "/take01.words.json", data: btoa('[{"w":"hi"}]') });
    native("media.write", { path: `${outside}.txt`, data: btoa("one file\n"), truncate: true });
    const sh = fresh();
    native("fs.place_link" as never, { id: sh.fs.id, path: "/media", target: outside } as never);
    native("fs.place_link" as never, { id: sh.fs.id, path: "/notes.txt", target: `${outside}.txt` } as never);
    expect((await sh.exec("cat media/take01.words.json")).stdout).toBe('[{"w":"hi"}]');
    expect((await sh.exec("ls media")).stdout).toBe("take01.words.json\n");
    expect((await sh.exec("wc -l < notes.txt")).stdout.trim()).toBe("1");
    expect((await sh.exec("cp media/take01.words.json copy.json && cat copy.json")).stdout).toBe('[{"w":"hi"}]');
    // never written through
    expect((await sh.exec("echo x > media/new.txt")).exitCode).not.toBe(0);
    expect((await sh.exec("echo x >> notes.txt")).exitCode).not.toBe(0);
    expect((await sh.exec("rm media/take01.words.json")).exitCode).not.toBe(0);
    expect(native("media.size", { path: `${outside}/take01.words.json` })).toBe(12);
    // no outside name shows, and the shell can't make a link out
    expect((await sh.exec("readlink -f media/take01.words.json")).stdout).toBe("/media/take01.words.json\n");
    expect((await sh.exec("readlink media")).stdout).toBe("(outside the project)\n");
    expect((await sh.exec("ln -s ../../../../../../etc evil")).exitCode).not.toBe(0);
    expect((await sh.exec("ln media/take01.words.json hard")).exitCode).not.toBe(0);
    // rdb's media verbs, run in process, open the linked file
    expect((await sh.exec("rdb media probe media/take01.words.json")).stderr).not.toContain("No such file");
  });
});

describe.skipIf(!engine)("a refused write through a link", () => {
  // just-bash (unmodified) checks a redirect's target with exists/stat and writes after the command, outside any
  // catch, so a write the filesystem refuses ends the whole script; bash would fail only that command. Until just-bash
  // opens redirect targets up front, the shell says so plainly (exit 1, the reason) and nothing behind the link changes.
  test("ends the command with exit 1 and the reason; nothing outside changes", async () => {
    const tmp = (native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp";
    const outside = `${tmp}raea-outside-chain-${Date.now()}`;
    const o = native("fs.open", { root: outside });
    native("fs.write", { id: o.id, path: "/link.json", data: btoa("{}") });
    const sh = fresh();
    native("fs.place_link" as never, { id: sh.fs.id, path: "/media", target: outside } as never);
    const r = await sh.exec("echo one; echo x >> media/link.json; echo after=$?");
    expect(r.exitCode).toBe(1);
    expect(r.stderr).toContain("EACCES");
    expect(native("media.size", { path: `${outside}/link.json` })).toBe(2);
  });
});
