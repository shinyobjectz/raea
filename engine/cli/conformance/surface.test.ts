// The agent's surface can't drift: what `raea help` lists, what the shell handles, and every `raea <command>` the
// agent's context mentions (the craft guides in sdk/guides, the skills, AGENTS.md) must agree.
// One source per fact: the native `raea help` (engine/cli/src/cli.rs, every verb from its own usage) is the list, so a
// prompt naming a command it doesn't list fails here.
import { describe, expect, test } from "bun:test";
import { spawnSync } from "child_process";
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from "fs";
import { tmpdir } from "os";
import { dirname, join } from "path";

const here = join(import.meta.dir, "..");
const raea = join(here, "../..");
// the native raea the verbs run (src/native.ts calls it for `raea media ...`) and whose help is the list: RAEA_BIN, else
// this checkout's build, else the one on PATH (said, since it may be someone else's build)
const built = join(raea, "target/release/raea");
const bin = process.env.RAEA_BIN ?? (existsSync(built) ? built : "raea");
if (bin === "raea") console.warn(`surface: no ${built}; testing the raea on PATH`);
const binPath = bin.includes("/") ? bin : (spawnSync("which", [bin], { encoding: "utf8" }).stdout ?? "").trim();
// the checkout the binary was built in (target/release/raea), else this one: an L1 export has no history of its own
const ownCheckout = existsSync(binPath) ? join(dirname(realpathSync(binPath)), "../..") : "";
const checkout = ownCheckout && existsSync(join(ownCheckout, "engine/cli/src/cli.rs")) ? ownCheckout : raea;
/** by when each help source (engine/cli's command line and the usage consts it prints) had its committed content, in ms; the binary is
 *  current if it was built after all of them. Two bounds, the earlier taken: the last commit's author time touching it
 *  (landing rebases a commit, which renews its commit time but not that), and, when the file matches HEAD, its mtime (a
 *  build before the commit saw it; a rewrite to the same content, a revert or a branch switch, only moves the mtime
 *  later). Another lane's uncommitted edit counts from the commit alone, so it can't fail this check, and a file HEAD
 *  doesn't have doesn't count. Undefined without git history (an L1 export uses the binary's own checkout) */
const rustIn = (dir: string) => readdirSync(join(checkout, dir), { recursive: true }).map(String).filter((f) => f.endsWith(".rs")).map((f) => `${dir}/${f}`);
const helpSources = ["runtime/react/src/scene.rs", ...rustIn("runtime/react/src/scene"), ...rustIn("engine/cli/src"), ...rustIn("apps/check/src"), ...rustIn("apps/game/src"), ...rustIn("apps/tools/src")];
const git = (...a: string[]) => spawnSync("git", ["-C", checkout, ...a], { encoding: "utf8" });
const committed = git("rev-parse", "-q", "--verify", "HEAD").status !== 0 ? undefined : Math.max(0, ...helpSources.map((f) => {
  if (git("cat-file", "-e", `HEAD:${f}`).status !== 0) return 0;
  const authored = Number(git("log", "-1", "--no-merges", "--format=%at", "HEAD", "--", f).stdout.trim()) * 1000;
  return git("diff", "--quiet", "HEAD", "--", f).status === 0 ? Math.min(authored, statSync(join(checkout, f)).mtimeMs) : authored;
}));
/** no binary at all: the help tests skip, loudly. One built before the help's sources were last committed prints stale
 *  help: that fails */
const missing = !binPath || !existsSync(binPath);
const stale = !missing && committed !== undefined && statSync(binPath).mtimeMs < committed;
if (!missing && committed === undefined) console.warn(`surface: no git history in ${checkout}; not checking that ${binPath} is current`);
if (missing) console.warn(`surface: SKIPPING the help tests: no raea binary (${built}, PATH); build raea-cli or set RAEA_BIN`);
const cli = readFileSync(join(here, "src/index.ts"), "utf8");

/** the native raea's help: the one list of commands */
const help = missing || stale ? "" : (spawnSync(bin, ["help"], { encoding: "utf8" }).stdout ?? "");
/** the commands it lists: each `raea NAME` (and the `| NAME` forms: add | remove | install) */
const listed = new Set([...help.matchAll(/raea ([a-z][\w-]*)/g), ...help.matchAll(/\| ([a-z][\w-]*) /g)].map((m) => m[1]).concat("help"));
/** the verbs the native raea hands to this shell (engine/cli verbs.rs SHELL), and the cases the shell's handler has */
const shellVerbs = JSON.parse(/const SHELL: &\[&str\] = &(\[[^\]]*\]);/.exec(readFileSync(join(raea, "engine/cli/src/verbs.rs"), "utf8"))![1]) as string[];
const handled = new Set([...cli.matchAll(/case "(\w+)":/g)].map((m) => m[1]).filter((c) => c !== "--help" && c !== "help"));

/** the hand-written text under dir: a skill's reference/ is the help itself, written by tools/skills.sh gen, and raea-skills'
 * _template is a skill's placeholder text */
function files(dir: string, out: string[] = []): string[] {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (lstatSync(p).isDirectory()) f !== "reference" && !f.startsWith("_") && files(p, out);
    else if (f.endsWith(".md")) out.push(p);
  }
  return out;
}

describe("raea's surface", () => {
  test.skipIf(missing)("the raea under test is current", () => {
    if (stale) throw new Error(`${binPath} is older than the help's sources as committed (engine/cli, sdk/react's motion, apps/check, apps/game, apps/tools in ${checkout}): rebuild raea-cli (lane build -- cargo build --release -p raea-cli), or set RAEA_BIN`);
  });

  test.skipIf(missing || stale)("the native raea hands the shell the verbs it handles, and lists them", () => {
    expect(listed.size).toBeGreaterThan(20);
    expect([...handled].filter((c) => !shellVerbs.includes(c) && c !== "add")).toEqual([]);
    expect(shellVerbs.filter((c) => !handled.has(c) || !listed.has(c))).toEqual([]);
  });

  test.skipIf(missing || stale)("an argument a verb doesn't take is refused and named, before any work", () => {
    // zsh's quoted "--frame 20" is one word: dropped, it rendered every frame
    const r = spawnSync(bin, ["render", "x.tsx", "-o", "x.png", "--frame 20"], { encoding: "utf8" });
    expect(r.status).not.toBe(0);
    expect(r.stderr + r.stdout).toContain(`raea render doesn't take "--frame 20"`);
    expect(r.stderr + r.stdout).toContain("usage: raea render EDIT.tsx");
  });

  test.skipIf(missing || stale)("every `raea <command>` in the agent's context exists", () => {
    const sources = [...files(join(raea, "sdk/guides")), ...files(join(raea, "skills/skills")), ...(existsSync(join(raea, "sdk/skills")) ? files(join(raea, "sdk/skills")) : []), "AGENTS.md (cli NOTES)"];
    const bad: string[] = [];
    for (const src of sources) {
      const text = src.startsWith("AGENTS.md") ? /const NOTES = `([\s\S]*?)`;/.exec(cli)![1] : readFileSync(src, "utf8");
      for (const m of text.matchAll(/`raea (\w+)/g)) if (!listed.has(m[1])) bad.push(`${src.replace(raea + "/", "")}: raea ${m[1]}`);
      // render's --at takes frame numbers (cut's and frame's take seconds)
      for (const m of text.matchAll(/raea render [^\n`]*?--at ([^\s`\\]+)/g)) if (!/^(\d+|[A-Z]+)(,(\d+|[A-Z]+))*$/.test(m[1])) bad.push(`${src.replace(raea + "/", "")}: raea render --at ${m[1]} (frame numbers)`);
    }
    expect(bad).toEqual([]);
  });

  test("every guide the agent's context names, `raea guide NAME` (or craft) finds", () => {
    // the native guide()'s rule (engine/cli/src/guide.rs): coverage (docs/sdk-coverage.md), its aliases, then
    // craft/NAME.md, modes/, verbs/, motion/
    const craft = join(raea, "sdk/guides/craft");
    const alias: Record<string, string> = { craft: "floor", router: "ROUTER" };
    const found = (n: string) =>
      (n === "coverage" && existsSync(join(raea, "docs/sdk-coverage.md"))) ||
      ["", "modes/", "verbs/", "motion/"].some((d) => {
        try {
          return lstatSync(join(craft, `${d}${alias[n] ?? n}.md`)).isFile();
        } catch {
          return false;
        }
      });
    const host = join(raea, "apps/app/host");
    const sources = [
      ...files(join(raea, "sdk/guides")),
      ...files(join(raea, "skills/skills")), ...(existsSync(join(raea, "sdk/skills")) ? files(join(raea, "sdk/skills")) : []),
      // the feed's prompts (apps/app is a submodule: an export without it checks the rest)
      ...(existsSync(host) ? readdirSync(host) : []).filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts")).map((f) => join(host, f)),
      "AGENTS.md (cli NOTES)",
      "raea help",
    ];
    const bad: string[] = [];
    for (const src of sources) {
      const text = src.startsWith("AGENTS.md") ? /const NOTES = `([\s\S]*?)`;/.exec(cli)![1] : src.startsWith("raea help") ? help : readFileSync(src, "utf8");
      const named = [...text.matchAll(/raea (?:guide|craft) ([a-z0-9][\w/-]*)/g), ...text.matchAll(/\["guide", "([\w/-]+)"/g)].map((m) => m[1]);
      for (const n of named) if (!/[A-Z]/.test(n) && !found(n)) bad.push(`${src.replace(raea + "/", "")}: ${n}`);
    }
    expect(bad).toEqual([]);
  });

  test("the verbs the native raea runs (src/native.ts) answer", () => {
    const dir = mkdtempSync(join(tmpdir(), "raea-init-"));
    const run = (...a: string[]) => spawnSync("bun", [join(here, "src/native.ts"), bin, ...a], { cwd: dir, encoding: "utf8" });
    try {
      expect(run("init").stdout).toContain("made edit.tsx, cuts.ts, styles.gen.ts, AGENTS.md");
      expect(["edit.tsx", "cuts.ts", "styles.gen.ts", "AGENTS.md", "components"].filter((f) => !existsSync(join(dir, f)))).toEqual([]);
      expect(run("init").stdout).toContain("nothing to make");
      expect(run("part", "add", "face-frame").stdout).toContain(`import FaceFrame from "./components/face-frame"`);
      expect(readFileSync(join(dir, "components/face-frame.tsx"), "utf8")).toContain("export default");
      expect(run("part", "add", "face-frame").stdout).toContain("already here");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
    // by feature (S5a): space.json, package.json, assets/, src/ with main.tsx, a feature, shots/; never components/
    const feat = mkdtempSync(join(tmpdir(), "raea-init-feature-"));
    const go = (...a: string[]) => spawnSync("bun", [join(here, "src/native.ts"), bin, ...a], { cwd: feat, encoding: "utf8" });
    try {
      expect(go("init", "--layout", "feature").stdout).toContain("made space.json, package.json, src/main.tsx");
      expect(["space.json", "package.json", "assets", "src/main.tsx", "src/title.tsx", "src/shots/opening.tsx", "src/screens", "src/ui", "AGENTS.md"].filter((f) => !existsSync(join(feat, f)))).toEqual([]);
      expect(existsSync(join(feat, "components"))).toBe(false);
      expect(go("init", "--layout", "feature").stdout).toContain("nothing to make");
      expect(go("init", "--layout", "other").status).toBe(2);
    } finally {
      rmSync(feat, { recursive: true, force: true });
    }
  });

  // a made clip (two colours: the take has a jump cut at 2 s) and a made-up transcript, never real footage
  test.skipIf(spawnSync("ffmpeg", ["-version"]).status !== 0)("raea cut cuts a take by its lines, and by --at", () => {
    const dir = mkdtempSync(join(tmpdir(), "raea-cut-"));
    const run = (...a: string[]) => spawnSync("bun", [join(here, "src/native.ts"), bin, ...a], { cwd: dir, encoding: "utf8" });
    try {
      mkdirSync(join(dir, "media"));
      const color = (c: string) => ["-f", "lavfi", "-i", `color=c=${c}:s=160x120:r=30:d=2`];
      spawnSync("ffmpeg", ["-v", "error", "-y", ...color("red"), ...color("blue"), "-filter_complex", "[0][1]concat=n=2:v=1:a=0", "-c:v", "libx264", "-pix_fmt", "yuv420p", join(dir, "media/take01.mp4")]);
      const said: [string, number, number][] = [["Hello", 200, 600], ["there.", 650, 1000], ["This", 1500, 1700], ["is", 1750, 1850], ["the", 1900, 2000], ["second", 2100, 2500], ["line.", 2550, 2900]];
      writeFileSync(join(dir, "media/take01.words.json"), JSON.stringify(said.map(([text, start_ms, end_ms]) => ({ text, start_ms, end_ms }))));
      const r = run("cut", "take01", "hello there", "the second line");
      expect(r.stderr).toBe("");
      const cuts = JSON.parse(readFileSync(join(dir, "cuts.json"), "utf8")).cuts;
      expect(cuts.map((c: { line: string }) => c.line)).toEqual(["Hello there.", "the second line."]);
      // in the pause before "the" (1.9 s), on the take's frame grid
      expect(cuts[1].from).toBeGreaterThan(1.85);
      expect(cuts[1].from).toBeLessThanOrEqual(1.9);
      expect(JSON.parse(readFileSync(join(dir, "words.json"), "utf8")).length).toBe(5);
      // the safe band from each clip's first frame (no face in a colour field)
      const band = { safe: { top: 220, bottom: 1500 } };
      expect(JSON.parse(readFileSync(join(dir, "perception.json"), "utf8"))).toEqual({ "0": band, "33": band });
      expect(readFileSync(join(dir, "cuts.ts"), "utf8")).toContain(`from "./media/take01.mp4"`);
      expect(run("cut", "take01", "--at", "0.5-1.5", "--append").stdout).toContain("3 clips");
      expect(run("cut", "take01", "never said here").stderr).toContain("isn't in the transcript");
      // a 4:3 take in a 9:16 edit with no face in it: ScreenFit, with its aspect
      const frame = run("frame", "take01", "--at", "1");
      expect([frame.stderr, frame.stdout]).toEqual(["", expect.stringContaining("<ScreenFit src={...} aspect={1.333} /> (raea part add screen-fit)")]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 60_000); // several encodes and face lookups: slow on a loaded machine

  // a made-up brand's site on localhost: raea brand reads its HTML and CSS itself (scrape.ts), no API or network
  const SITE = {
    "/": `<!doctype html><html lang="en"><head><title>Acme | Made up</title><meta name="description" content="Acme makes things up.">
<link rel="stylesheet" href="/site.css"><link rel="icon" type="image/svg+xml" href="/icon.svg"></head><body>
<header><a class="logo" href="/"><svg viewBox="0 0 120 40"><rect width="120" height="40"/></svg></a></header>
<main><h1>Acme makes it up</h1><p>${"Every word here is made up for a test, and there are enough of them to read as a real page. ".repeat(8)}</p>
<a class="btn btn-primary" href="/start">Start free</a></main></body></html>`,
    "/site.css": `@import url("https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@700&display=swap");
:root { --brand: #c3f53c; --background: #0b0b0f; --foreground: #f5f5f5; --font-body: "Proxima Nova", sans-serif; --radius: 16px; }
@media (prefers-color-scheme: dark) { :root { --foreground: #ff00ff; } }
body { background: var(--background); color: var(--foreground); font-family: var(--font-body); font-size: 1rem; line-height: 1.5; }
h1 { font-family: "Space Grotesk", sans-serif; font-size: clamp(2rem, 5vw, 3.5rem); font-weight: 700; letter-spacing: -0.02em; }
h2 { font-family: "Space Grotesk", sans-serif; font-size: 2rem; font-weight: 700; }
.btn-primary { background-color: var(--brand); color: #0b0b0f; border-radius: 999px; padding: 12px 24px; font-weight: 600; }
.btn-primary:hover { background-color: #ffffff; }
.card { border-radius: var(--radius); background: #16161d; }
${Array.from({ length: 40 }, (_, i) => `.u${i} { margin: ${i}px; }`).join("\n")}`,
    "/icon.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><circle r="16" cx="16" cy="16" fill="#c3f53c"/></svg>`,
    // a page its script draws: nothing for a plain fetch to read
    "/app": `<!doctype html><html><head><title>Acme App</title></head><body><div id="root"></div><script src="/app.js"></script></body></html>`,
  } as Record<string, string>;
  const brandRun = async (url: string) => {
    const dir = mkdtempSync(join(tmpdir(), "raea-brand-"));
    const p = Bun.spawn(["bun", join(here, "src/native.ts"), bin, "brand", url], { cwd: dir, stdout: "pipe", stderr: "pipe" });
    const [out, err] = [await new Response(p.stdout).text(), await new Response(p.stderr).text()];
    return { dir, out, err, code: await p.exited };
  };

  test("raea brand reads the kit from the brand's own site", async () => {
    const server = Bun.serve({ port: 0, fetch: (req) => (SITE[new URL(req.url).pathname] !== undefined ? new Response(SITE[new URL(req.url).pathname], { headers: { "content-type": new URL(req.url).pathname.endsWith(".css") ? "text/css" : new URL(req.url).pathname.endsWith(".svg") ? "image/svg+xml" : "text/html" } }) : new Response("no", { status: 404 })) });
    const { dir, out, err, code } = await brandRun(`http://localhost:${server.port}/`);
    try {
      expect([code, err]).toEqual([0, ""]);
      expect(out).toContain("Acme (localhost)");
      expect(out).not.toContain("WARNING");
      const kit = JSON.parse(readFileSync(join(dir, "brand/brand.json"), "utf8"));
      // the page's own variables by role; the dark scheme's @media isn't the page as it is
      expect(kit.palette).toEqual({ primary: "#c3f53c", accent: "#c3f53c", surface: "#0b0b0f", text_on_surface: "#f5f5f5", text_on_accent: "#0b0b0f" });
      expect(kit.fonts).toEqual({ heading: { kit: "Space Grotesk", site: "Space Grotesk" }, body: { kit: "Montserrat", site: "Proxima Nova" } });
      expect([kit.scale.h1.size, kit.scale.h1.tracking, kit.scale.p.size, kit.button.bg, kit.button.radius, kit.radius]).toEqual([56, -0.3, 16, "#c3f53c", 999, 16]);
      expect(kit.logos.map((l: any) => [l.file, l.type, l.width, l.height])).toEqual([
        ["media/brand/localhost-logo.svg", "logo", 120, 40],
        ["media/brand/localhost-favicon.svg", "favicon", 32, 32],
      ]);
      expect(readFileSync(join(dir, kit.logos[0].file), "utf8")).toStartWith('<svg xmlns="http://www.w3.org/2000/svg"');
      // the variables engine/cli/template/theme.css maps the brand classes onto
      const css = readFileSync(join(dir, "brand/brand.css"), "utf8");
      for (const v of ["--brand-accent", "--brand-background", "--brand-text", "--brand-color-1", "--brand-font-heading", "--brand-font", "--brand-h1-size", "--brand-h2-size", "--brand-h4-size", "--brand-p-size", "--brand-radius"])
        expect(css).toContain(`${v}:`);
      expect(css).not.toContain("Proxima Nova");
      expect(readFileSync(join(dir, "BRAND.md"), "utf8")).toContain("- accent: #c3f53c (--brand-accent)");
    } finally {
      server.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("raea brand refuses a site a fetch can't read, and says to read it in a browser", async () => {
    const server = Bun.serve({ port: 0, fetch: (req) => new Response(SITE[new URL(req.url).pathname] ?? "no", { status: SITE[new URL(req.url).pathname] ? 200 : 404 }) });
    const { dir, out, err, code } = await brandRun(`http://localhost:${server.port}/app`);
    try {
      expect(code).not.toBe(0);
      expect(out + err).toContain("open the site in your browser tool");
      const md = readFileSync(join(dir, "BRAND.md"), "utf8");
      expect(md).toContain("## The fetch was thin: check this kit in a browser");
      expect(md).toContain("characters of text without JavaScript");
      expect(JSON.parse(readFileSync(join(dir, "brand/brand.json"), "utf8")).source.thin.length).toBeGreaterThan(0);
    } finally {
      server.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  // a stock talking take (fixtures/CREDITS.md): the face branch of frame and perception.json. The footage fixtures are
  // the maintainers' (tools/public-first/private.paths), so a checkout without them skips this
  test.skipIf(!existsSync(join(raea, "engine/media/fixtures")))("raea frame and raea cut find the face in a talking take", () => {
    const dir = mkdtempSync(join(tmpdir(), "raea-face-"));
    const run = (...a: string[]) => spawnSync("bun", [join(here, "src/native.ts"), bin, ...a], { cwd: dir, encoding: "utf8" });
    try {
      mkdirSync(join(dir, "media"));
      writeFileSync(join(dir, "media/face.mp4"), readFileSync(join(here, "conformance/fixtures/face.mp4")));
      const frame = run("frame", "face", "--at", "1");
      expect([frame.stderr, frame.stdout]).toEqual(["", expect.stringMatching(/a talking take\. <FaceFrame src={\.\.\.} \/> \(raea part add face-frame\) finds the face/)]);
      expect(run("cut", "face", "--at", "0.5-2.5").stderr).toBe("");
      // 320x180 covering 1080x1920 is 10.67x: the face, about half the take's height, fills about half the edit's
      const { "0": seen } = JSON.parse(readFileSync(join(dir, "perception.json"), "utf8"));
      expect(seen.safe).toEqual({ top: 220, bottom: 1500 });
      const { x, y, w, h } = seen.face;
      expect(h).toBeGreaterThan(700);
      expect(h).toBeLessThan(1100);
      expect(x + w / 2).toBeGreaterThan(250);
      expect(x + w / 2).toBeLessThan(650);
      expect(y + h / 2).toBeGreaterThan(600);
      expect(y + h / 2).toBeLessThan(1300);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 60_000); // several encodes and face lookups: slow on a loaded machine
});
