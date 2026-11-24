// `raea show` refuses a render that still holds an error (docs/stage-guards.feature SG15): the newest render that wrote
// the file, from its project's record, less what that render let through and what --despite names. The record is
// faked in memory; parseShow's side is the --despite flag itself.
import { describe, expect, test } from "bun:test";
import { held, openErrors } from "../src/held.ts";
import { parseShow } from "../src/show.ts";

const P = "/work/ads";
const error = (check: string, says = `${check} says`) => ({ check, level: "error", says });

/** A project at P whose record holds these entries, as core/facts/src/record.rs writes them. */
function records(entries: Record<string, unknown>[]) {
  const files = new Map<string, string>([[`${P}/.raea/project`, ""], [`${P}/.raea/record`, ""]]);
  entries.forEach((e, i) => files.set(`${P}/.raea/record/2026112${i}-000000-film.json`, JSON.stringify(e)));
  return {
    exists: (p: string) => files.has(p),
    read: (p: string) => files.get(p) ?? "",
    list: (d: string) => [...files.keys()].filter((k) => k.startsWith(`${d}/`)).map((k) => k.slice(d.length + 1)),
  };
}

describe("SG15: a broken render isn't shown", () => {
  test("the newest render that wrote the file holds an error: refused, the error named", () => {
    const h = records([
      { at: 1, by: "raea render", out: "out/film.mp4", findings: [], despite: [] },
      { at: 2, by: "raea render", out: "out/film.mp4", findings: [error("type-small", "'Reviews' is 16 px tall"), { check: "exposure", level: "warn", says: "" }], despite: [] },
    ]);
    const why = held([`${P}/out/film.mp4`], P, [], h);
    expect(why).toContain("film.mp4: type-small: 'Reviews' is 16 px tall");
    expect(why).toContain(`--despite type-small "why"`);
    expect(why).not.toContain("exposure");
  });

  test("a later clean render clears it, and an older one doesn't count", () => {
    const h = records([
      { at: 2, by: "raea render", out: "out/film.mp4", findings: [error("camera-lurch")], despite: [] },
      { at: 3, by: "raea render", out: "out/film.mp4", findings: [], despite: [] },
    ]);
    expect(held([`${P}/out/film.mp4`], P, [], h)).toBeUndefined();
  });

  test("let through at the render, or with --despite: shown", () => {
    const at = records([{ at: 1, by: "raea render", out: "out/film.mp4", findings: [error("frame-empty")], despite: [{ check: "frame-empty", why: "a black beat" }] }]);
    expect(held([`${P}/out/film.mp4`], P, [], at)).toBeUndefined();
    const h = records([{ at: 1, by: "raea render", out: "out/film.mp4", findings: [error("part-snap"), error("type-angle")], despite: [] }]);
    expect(openErrors([`${P}/out/film.mp4`], P, [{ check: "part-snap", why: "a cut on the beat" }], h)).toEqual(["film.mp4: type-angle: type-angle says"]);
  });

  test("a still from a frames render is that render's; a file no render wrote is clear", () => {
    const h = records([{ at: 1, by: "raea render", out: "out/frames", findings: [error("type-reveal")], despite: [] }]);
    expect(held([`${P}/out/frames/frame-00045.png`], P, [], h)).toContain("frame-00045.png: type-reveal");
    expect(held([`${P}/assets/photo.jpg`, "/tmp/elsewhere.mp4"], "/tmp", [], h)).toBeUndefined();
    // a check's entry isn't a render's
    const checked = records([{ at: 1, by: "raea check", out: "out/film.mp4", findings: [error("clipped")], despite: [] }]);
    expect(held([`${P}/out/film.mp4`], P, [], checked)).toBeUndefined();
  });

  test("an absolute out, as a render outside the project writes it", () => {
    const h = records([{ at: 1, by: "raea render", out: "/tmp/scratch/a.mp4", findings: [error("subject-cut")], despite: [] }]);
    expect(held(["/tmp/scratch/a.mp4"], P, [], h)).toContain("a.mp4: subject-cut");
  });

  test("--despite takes the check and why, and neither is a file", () => {
    const a = parseShow(["out/film.mp4", "--despite", "type-small", "a sign seen in passing", "--title", "Cut"], P);
    expect(a.files).toEqual([`${P}/out/film.mp4`]);
    expect(a.despite).toEqual([{ check: "type-small", why: "a sign seen in passing" }]);
    expect(() => parseShow(["out/film.mp4", "--despite", "type-small"], P)).toThrow("--despite takes the check and why");
  });
});
