import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { load, run, status } from "../src/project";

/** An object's folder as void makes it: space.json, edit.tsx, cuts.ts, media/ */
const object = (dir: string, slug: string, name = slug) => {
  mkdirSync(join(dir, "media"), { recursive: true });
  writeFileSync(join(dir, "space.json"), JSON.stringify({ name, slug, created: 0, opened: 0, hue: 0 }));
  writeFileSync(join(dir, "edit.tsx"), "export default 1;\n");
  writeFileSync(join(dir, "cuts.ts"), "export const cuts = [];\n");
  writeFileSync(join(dir, "media", "take.mov"), "not tracked");
  return dir;
};
const lone = () => object(mkdtempSync(join(tmpdir(), "raea-project-")), "ad", "Ferro ad");
const workspace = () => {
  const ws = mkdtempSync(join(tmpdir(), "raea-ws-"));
  object(join(ws, "objects", "ad"), "ad", "Ferro ad");
  object(join(ws, "objects", "pong"), "pong", "Pong");
  return ws;
};
const $ = (root: string, ...a: string[]) => run([a[0], root, ...a.slice(1)]);

describe("objects", () => {
  test("a workspace's objects are its folders; one is on screen; each has a kind kept in its space.json", () => {
    const ws = workspace();
    expect(load(ws).objects.map((o) => o.name)).toEqual(["ad", "pong"]);
    expect($(ws, "object", "kind", "pong", "game").code).toBe(0);
    expect($(ws, "object", "kind", "pong", "opera").code).toBe(2);
    expect(JSON.parse(readFileSync(join(ws, "objects/pong/space.json"), "utf8"))).toMatchObject({ name: "Pong", slug: "pong", kind: "game" });
    expect($(ws, "object", "open", "Pong").code).toBe(0); // by title too
    const pr = load(ws);
    expect(pr.current).toBe("pong");
    expect(pr.objects.find((o) => o.name === "pong")!.kind).toBe("game");
    expect($(ws, "object", "open", "nope").code).toBe(1);
    expect(status(ws)).toContain("* pong (game");
    rmSync(ws, { recursive: true });
  });

  test("a folder with no objects in it is one object itself (an eval's space)", () => {
    const s = lone();
    expect(load(s).objects.map((o) => [o.name, o.dir])).toEqual([["ad", s]]);
    rmSync(s, { recursive: true });
  });
});

describe("tasks and questions", () => {
  test("a question is answered by choosing or in the person's words; the task waits until then", () => {
    const ws = workspace();
    expect($(ws, "task", "new", "make the hook bigger", "--object", "ad").text).toContain("t1");
    expect(load(ws).tasks[0]).toMatchObject({ object: "ad", state: "working" });
    expect($(ws, "ask", "t1", "Which ending?", "--option", "end card: the logo").code).toBe(2); // two options at least
    expect($(ws, "ask", "t1", "Which ending?", "--option", "end card: the logo", "--option", "last word: cut on the line").out).toMatchObject({ id: "q1" });
    expect(load(ws).tasks[0].state).toBe("needs input");
    expect(status(ws)).toContain("waiting on q1: Which ending?");
    expect($(ws, "answer", "q1", "fade").code).toBe(2);
    expect($(ws, "answer", "q1", "end card, last word").code).toBe(2); // one answer
    expect($(ws, "answer", "q1", "other: end on her laugh").code).toBe(0);
    const pr = load(ws);
    expect(pr.questions[0]).toMatchObject({ answer: ["Other"], other: "end on her laugh" });
    expect(pr.tasks[0].state).toBe("working");
    expect($(ws, "task", "object", "t1", "pong").code).toBe(0);
    expect(load(ws).tasks.map((t) => t.object)).toEqual(["pong"]);
    rmSync(ws, { recursive: true });
  });
});

describe("versions", () => {
  test("a render saves a version of its object with its render kept; unchanged gives it back; diff, restore, checkout", () => {
    const ws = workspace(), s = join(ws, "objects/ad");
    writeFileSync(join(s, "final.mp4"), "frames v1");
    const v1 = $(ws, "save", "--object", "ad", "--render", "final.mp4", "--message", "first cut");
    expect(v1.text).toContain("saved v1 of ad");
    expect((v1.out as any).files).toBe(2); // edit.tsx, cuts.ts: media, renders and space.json aren't tracked
    expect($(ws, "save", "--object", "ad", "--render", "final.mp4").text).toContain("the same as v1");
    writeFileSync(join(s, "edit.tsx"), "export default 3;\n"); // the person's own edit, outside the shell
    writeFileSync(join(s, "hook.tsx"), "export default 4;\n");
    writeFileSync(join(s, "final.mp4"), "frames v2");
    expect($(ws, "save", "--object", "ad", "--render", "final.mp4").text).toContain("saved v2");
    expect($(ws, "diff", "1", "2", "--object", "ad").text.split("\n")).toEqual(["+ /hook.tsx", "~ /edit.tsx", "~ the render"]);
    expect($(ws, "restore", "1", "--object", "ad", "--paths").text.split("\n").sort()).toEqual(["/edit.tsx", "/hook.tsx"]);
    expect($(ws, "restore", "1", "--object", "ad").code).toBe(0);
    expect(readFileSync(join(s, "edit.tsx"), "utf8")).toBe("export default 1;\n");
    expect(existsSync(join(s, "hook.tsx"))).toBe(false);
    expect(existsSync(join(s, "media", "take.mov"))).toBe(true);
    // pong's versions are its own
    expect($(ws, "versions", "--object", "pong").text).toBe("no versions of pong yet");
    // v2 is still there to go back to, and its render plays without touching the object
    const out = mkdtempSync(join(tmpdir(), "raea-checkout-"));
    expect($(ws, "checkout", "2", out, "--object", "ad").code).toBe(0);
    expect(readFileSync(join(out, "hook.tsx"), "utf8")).toBe("export default 4;\n");
    expect(readFileSync(join(out, "final.mp4"), "utf8")).toBe("frames v2");
    rmSync(ws, { recursive: true }); rmSync(out, { recursive: true });
  });
});

describe("the check policy", () => {
  test("ready needs a full check, every failure fixed or waived in writing, every acceptance rule measured or waived", () => {
    const s = lone();
    $(s, "task", "new", "a 15 s ad", "--kind", "new object", "--accept", "! the length is within the format", "--accept", "! Kelly approves the claims");
    expect($(s, "task", "state", "t1", "ready").text).toContain("no version of ad");
    writeFileSync(join(s, "final.mp4"), "x");
    writeFileSync(join(s, "check.json"), JSON.stringify({ full: false, results: [
      { rule: "the length is within the format", status: "pass" }, { rule: "nothing covers the face", status: "fail" }, { rule: "no black frames", status: "not measured" }] }));
    $(s, "save", "--render", "final.mp4", "--check", "check.json");
    let r = $(s, "task", "state", "t1", "ready");
    expect(r.code).toBe(1);
    expect(r.text).toContain("fast check only");
    // a full check of the same render goes on the same version
    expect($(s, "save", "--render", "final.mp4", "--check", "check.json", "--full").text).toContain("v1");
    r = $(s, "task", "state", "t1", "ready");
    expect(r.text).toContain("fails: nothing covers the face");
    expect(r.text).toContain("not measured: ! Kelly approves the claims");
    expect($(s, "task", "state", "t1", "ready", "--waive", "nothing covers the face").code).toBe(2); // a waiver says why
    r = $(s, "task", "state", "t1", "ready", "--waive", "nothing covers the face: the judge read her hand as text", "--waive", "Kelly approves the claims: Kelly approved by email 10-17");
    expect(r.code).toBe(0);
    expect(status(s)).toMatch(/ad \(video, "Ferro ad"\): ready, v1/);
    // a new version is a draft again until it's checked and marked
    writeFileSync(join(s, "edit.tsx"), "export default 9;\n");
    $(s, "save", "--render", "final.mp4");
    expect(status(s)).toMatch(/ad \(video, "Ferro ad"\): draft, v2/);
    rmSync(s, { recursive: true });
  });
});
