// @raea/cli: `raea`, the agent's one command for editing video as a React + TypeScript project (docs: the plan
// "TypeScript-only editing", 2026-10-17). It runs inside @raea/shell on raea's V8, the same on the Mac, the iPhone and a
// runner. What's portable is done here: the project's files (init, add, cuts.ts). What needs perception, ffmpeg or
// the renderer is asked of the host (void on the Mac, the app on the iPhone) through `host`, which answers in text or
// JSON. Every command prints what a person would read; files hold the rest.
import { REGISTRY, TEMPLATE } from "./registry.gen";

/** The host's answer to an op: void (Rust) or the app runs it and replies. Throws when there's no host. */
export type Host = (op: string, args: Record<string, unknown>) => any;

type Fs = {
  exists(p: string): Promise<boolean>;
  readFile(p: string, enc?: string): Promise<string>;
  writeFile(p: string, data: string): Promise<void>;
  mkdir(p: string, o?: { recursive?: boolean }): Promise<void>;
};
type Ctx = { cwd: string; fs: Fs };
const out = (stdout: string, exitCode = 0, stderr = "") => ({ stdout, stderr, exitCode });

export const HELP = `raea: edit a video as a React + TypeScript project (edit.tsx, cuts.ts, components/, brand/brand.css)

  raea init                         the project's files where they're missing (edit.tsx, cuts.ts, AGENTS.md)
  raea brand DOMAIN|NAME            the brand's kit: brand/brand.css (its colours, type, sizes), logos in media/brand
  raea media                        the takes: length, shape, what's said (timed sentences), a contact sheet each
  raea script [NAME.brief]          compile a brief into edit.tsx (every piece when no name is given)
  raea cut TAKE "line" "line" ...   cuts.ts: clips cut in the pauses around each line, in order (--append adds to it)
  raea cut TAKE --at 3.2-7.5 ...    clips at those seconds of the take (one --at each): a take with no words (a screen
                                    recording, b-roll) or an exact moment; mixes with lines, --append adds
  raea frame TAKE [--at S]          where the face is, and <FaceFrame> or <ScreenFit> for this take
  raea list                         the components you can add
  raea add NAME ...                 copy components into components/ (yours to change; --force replaces)
  raea render [NAME]                final.mp4 from edit.tsx (NAME: a brief's piece, NAME.tsx to NAME.mp4)
  raea check [NAME] [--full]        render, then measure: cuts, endings, text, faces; each fault with its fix.
                                    --full also asks the model checks (framing, captions, the message), slower
  raea still S [S ...]              frames of the render as PNGs (then: look at them)
  raea sheet                        a contact sheet of the render
  raea craft [NAME]                 the craft library: stages, modes, playbooks (no name lists them)
  raea skills                       the skills on this computer, and the person's own commands
  raea skill NAME [FILE]            a skill's instructions whole (or a file it brings): use it when the request fits
  raea help

The project (the folder you work in) holds objects, each anything that renders: this folder is one. Its record:
  raea status                       the project as it stands: objects, open tasks, questions, notes
  raea purpose "TEXT"               what the project is for
  raea objects                      each object: its kind, draft or ready, its last version (* on screen)
  raea object open|kind SLUG [KIND] put an object on screen; say what it is (video game deck document music motion scene3d still)
  raea task new "WORDS" [--kind K]  the request as a task (kind: new object, change, quick edit, question);
                                    --accept "! rule" for what the person said it must do
  raea task step ID "TEXT" [--done] a step of its plan, and when it's done
  raea task state ID STATE          planned, working, needs input, draft, ready, delivered. ready is refused until the
                                    last version passed raea check --full, or each failure is --waive "rule: reason"
  raea task show [ID]               a task: its steps, questions, versions, and what stands between it and ready
  raea tasks                        every task and its state
  raea ask ID "QUESTION" --option "LABEL: what it means" ...   a question the person answers by choosing (2 to 4
                                    options, --multi for several; they can always answer in their own words). Then
                                    end your turn: the answer comes as their next message
  raea question [QID]               the open question (or QID) and its options
  raea answer QID "LABEL"           a question answered ("other: their words" for their own)
  raea note "TEXT"                  what the person decided or prefers, kept for the project
  raea notes                        the project's notes
  raea save [--message M]           a version of this object now (raea render saves one each time, raea check puts
                                    its result on it)
  raea versions                     this object's versions
  raea diff A B                     what changed between two of them
  raea checkout N DIR               version N's files and render into DIR (this object untouched)
  raea restore N                    this object back as version N (undo takes it back)

Sound is generated where the edit uses it: <Audio src={lyria("prompt", {seconds})} /> or sao("prompt", {seconds}).
`;

function flag(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

const clean = (args: string[], ...withValue: string[]) =>
  args.filter((a, i) => !a.startsWith("--") && !withValue.includes(args[i - 1] ?? ""));

/** A clip of a take: seconds in the take's own time, and the line it carries. */
export interface Cut {
  take: string;
  file: string;
  from: number;
  to: number;
  line: string;
}

/** cuts.ts from the clips: data the edit imports (each take's file as a module, so the bundler finds the media). */
export function cutsModule(clips: Cut[], edit: { width: number; height: number; fps: number }): string {
  const files = [...new Set(clips.map((c) => c.file))];
  const id = (f: string) => "m_" + f.replace(/\W/g, "_");
  const rows = clips.map((c) => `  { take: ${JSON.stringify(c.take)}, src: ${id(c.file)}, from: ${c.from.toFixed(3)}, to: ${c.to.toFixed(3)}, line: ${JSON.stringify(c.line)} },`);
  return `// written by \`raea cut\`: the clips, in order, in each take's own seconds. Re-run raea cut to change them, or edit
// by hand (the edit reads only this file). cuts.json beside it is the same list for raea's checks.
${files.map((f) => `import ${id(f)} from ${JSON.stringify("./" + f)};`).join("\n")}

export const edit = ${JSON.stringify(edit)};
export type Cut = { take: string; src: string; from: number; to: number; line: string };
export const cuts: Cut[] = [
${rows.join("\n")}
];
/** a clip's length in the edit's frames */
export const frames = (c: Cut) => Math.max(1, Math.round((c.to - c.from) * edit.fps));
/** the cuts end to end, in frames */
export const total = cuts.reduce((n, c) => n + frames(c), 0);
`;
}

const NOTES = `# This project

A video edit as a React + TypeScript project, rendered by raea.

- edit.tsx: the composition. Change anything: layout, timing, which component plays when.
- cuts.ts: the clips (\`raea cut\` writes it, with cuts.json and words.json beside it; re-run it rather than editing
  by hand, or captions and checks lose their timing). components/: parts you added (\`raea add\`), yours to change.
- brand/brand.css: the brand's tokens (\`raea brand DOMAIN\` fetches them). edit.tsx loads them with the project's
  Tailwind: \`<style>{css}</style>\`, css from "./styles.gen" (raea render writes it). Style only with the brand
  classes: colour text-brand-accent, bg-brand-accent, bg-brand-surface (bg-brand-bg), text-brand-on-surface,
  text-brand-on-accent, text-brand-ink, brand-1/2/3; face font-brand, font-brand-body; size text-display,
  text-title, text-caption, text-body, text-label (scaled for a 1080-wide frame); corners rounded-brand. Where a
  class can't reach, the --brand-* variables. Never a colour, font or size of your own.
- \`raea check\` renders and says what's wrong; \`raea still 3.5\` then look at the frame. \`raea craft\` has the
  craft (\`raea craft motion\` for writing the TSX); \`raea help\` lists the rest.

## @raea/motion (everything there is; import from "@raea/motion")

Time: useCurrentFrame() (frames, from 0 inside the nearest Sequence), useVideoConfig() {width, height, fps,
durationInFrames}, interpolate(frame, [in...], [out...], {extrapolateLeft, extrapolateRight: "clamp", easing}),
spring({frame, fps, config}), Easing.bezier(a, b, c, d) / Easing.inOut(...).
Layout: <AbsoluteFill>, <Sequence from durationInFrames>, <Series><Series.Sequence durationInFrames>, <Loop>, <Freeze>.
Media: <Video src startFrom endAt volume playbackRate muted framing="face" punchIn style>, <Audio src startFrom endAt
volume={n | (f) => n} gainDb fadeIn fadeOut>, <Img src>. Media files import as modules: import take from
"./media/x.mov". Generated sound: lyria("prompt", {seconds}) for music, sao("prompt", {seconds}) for an effect, as an
<Audio src>. <Mix lufs ceiling> masters the sound.
Words: useWords() -> [{text, from, to}] (frames, the cuts' words), word("phrase") -> the frame it's said,
findPhrase(words, "phrase", {occurrence, edge: "start" | "end"}).
Placement: <Region name="top" | "bottom" | "center" | "auto" | "full"> keeps a part in a band clear of the face and
the platforms' buttons; useRegion() gives its box.
Composition: registerRoot(() => <Composition id component width height fps durationInFrames />).
Not there: React hooks come from "react" (useMemo, useState), not from @raea/motion.
`;

/** The `raea` command's handler (the shell wraps it with just-bash's defineCommand, so this package needs no dependency). */
export function raeaHandler(host?: Host) {
  return async (args: string[], ctx: unknown) => {
    const c = ctx as unknown as Ctx;
    const p = (rel: string) => (rel.startsWith("/") ? rel : `${c.cwd.replace(/\/$/, "")}/${rel}`);
    const ask = (op: string, a: Record<string, unknown> = {}) => {
      if (!host) throw new Error(`raea ${op} needs a host (void on the Mac, the app on the iPhone); this shell has none`);
      return host(op, a);
    };
    const text = (v: unknown) => (typeof v === "string" ? v : JSON.stringify(v, null, 2)) + "\n";
    try {
      switch (args[0]) {
        case undefined:
        case "help":
        case "--help":
          return out(HELP);
        case "init": {
          const made: string[] = [];
          const put = async (name: string, body: string) => {
            if (await c.fs.exists(p(name))) return;
            await c.fs.writeFile(p(name), body);
            made.push(name);
          };
          const shape = ask("shape", {}) as { width: number; height: number; fps: number };
          await c.fs.mkdir(p("components"), { recursive: true });
          await put("edit.tsx", TEMPLATE["edit.tsx"]);
          await put("cuts.ts", cutsModule([], shape));
          await put("styles.gen.ts", `export const css = "";\n`);
          // AGENTS.md is raea's own notes: an older generated copy (it opens with this header) is brought up to date, a
          // person's own file is left alone
          const notes = p("AGENTS.md");
          if (await c.fs.exists(notes)) {
            const was = await c.fs.readFile(notes, "utf8");
            if (was !== NOTES && was.startsWith("# This project\n\nA video edit as a React + TypeScript project")) {
              await c.fs.writeFile(notes, NOTES);
              made.push("AGENTS.md (updated)");
            }
          } else await put("AGENTS.md", NOTES);
          return out(made.length ? `made ${made.join(", ")}\n` : "nothing to make: the project's files are here\n");
        }
        case "list":
          return out(
            Object.entries(REGISTRY)
              .map(([f, src]) => `${f.replace(/\.tsx$/, "").padEnd(14)} ${(/^\/\/ raea add [\w-]+: (.*)$/m.exec(src)?.[1] ?? "").split(". ")[0]}`)
              .join("\n") + "\n",
          );
        case "add": {
          const names = clean(args.slice(1));
          if (!names.length) return out("", 2, "raea add NAME ...  (raea list shows them)\n");
          const force = args.includes("--force");
          await c.fs.mkdir(p("components"), { recursive: true });
          const said: string[] = [];
          for (const n of names) {
            const src = REGISTRY[`${n}.tsx`];
            if (!src) return out(said.join(""), 1, `raea add: no component "${n}" (raea list shows them)\n`);
            const dest = p(`components/${n}.tsx`);
            if (!force && (await c.fs.exists(dest))) {
              said.push(`components/${n}.tsx is already here (yours now; --force replaces it)\n`);
              continue;
            }
            await c.fs.writeFile(dest, src);
            said.push(`components/${n}.tsx  import it: import ${n.replace(/(^|-)(\w)/g, (_, __, ch) => ch.toUpperCase())} from "./components/${n}";\n`);
          }
          return out(said.join(""));
        }
        case "cut": {
          const spans: number[][] = [];
          args.forEach((a, i) => {
            if (a === "--at" && args[i + 1]) {
              const m = /^(\d+(?:\.\d+)?)\s*-\s*(\d+(?:\.\d+)?)$/.exec(args[i + 1]);
              if (m) spans.push([Number(m[1]), Number(m[2])]);
            }
          });
          const [take, ...lines] = clean(args.slice(1), "--at");
          if (!take || (!lines.length && !spans.length)) return out("", 2, `raea cut TAKE "line" ["line" ...] | --at A-B [--at C-D ...] [--append]\n`);
          const prev: Cut[] = args.includes("--append") && (await c.fs.exists(p("cuts.json"))) ? JSON.parse(await c.fs.readFile(p("cuts.json"), "utf8")).cuts : [];
          const r = ask("cut", { take, lines, spans, keep: prev }) as { edit: { width: number; height: number; fps: number }; cuts: Cut[]; words: unknown[]; said: string };
          await c.fs.writeFile(p("cuts.ts"), cutsModule(r.cuts, r.edit));
          await c.fs.writeFile(p("cuts.json"), JSON.stringify({ edit: r.edit, cuts: r.cuts }, null, 2) + "\n");
          await c.fs.writeFile(p("words.json"), JSON.stringify(r.words) + "\n");
          return out(r.said.endsWith("\n") ? r.said : r.said + "\n");
        }
        case "script": {
          const file = clean(args.slice(1))[0];
          return out(text(ask("script", file ? { file } : {})));
        }
        case "frame": {
          const take = clean(args.slice(1), "--at")[0];
          if (!take) return out("", 2, "raea frame TAKE [--at SECONDS]\n");
          const at = flag(args, "--at");
          return out(text(ask("frame", { take, at: at === undefined ? undefined : Number(at) })));
        }
        case "brand": {
          const q = clean(args.slice(1)).join(" ");
          if (!q) return out("", 2, "raea brand DOMAIN|NAME  (alphalion.com, or a name to search)\n");
          return out(text(ask("brand", { brand: q })));
        }
        case "media":
          return out(text(ask("media", { add: args[1] === "add" ? args.slice(2) : [] })));
        case "render":
        case "check": {
          const piece = clean(args.slice(1))[0];
          const full = args[0] === "check" && args.includes("--full");
          return out(text(ask(args[0], { ...(piece ? { piece } : {}), ...(full ? { full } : {}) })));
        }
        case "still": {
          const at = clean(args.slice(1)).map(Number).filter((n) => Number.isFinite(n));
          if (!at.length) return out("", 2, "raea still SECONDS [SECONDS ...]\n");
          return out(text(ask("still", { at })));
        }
        case "sheet":
          return out(text(ask("sheet", {})));
        case "status":
        case "purpose":
        case "objects":
        case "object":
        case "task":
        case "tasks":
        case "ask":
        case "question":
        case "answer":
        case "note":
        case "notes":
        case "save":
        case "versions":
        case "diff":
        case "checkout": {
          const r = ask("project", { argv: args }) as { text: string; code: number };
          return r.code === 2 ? out("", 2, `raea ${r.text}\n`) : out(r.text + "\n", r.code);
        }
        case "restore": {
          // the files the restore writes, kept in this command's row first: the host writes them, and undo takes it back
          const plan = ask("project", { argv: [...args, "--paths"], json: true }) as { text: string; code: number };
          if (plan.code) return out("", plan.code, `raea restore: ${plan.text}\n`);
          const fs = c.fs as Fs & { touch?: (p: string) => void };
          for (const path of Object.keys(JSON.parse(plan.text))) fs.touch?.(path);
          const r = ask("project", { argv: args }) as { text: string; code: number };
          return out(r.text + "\n", r.code);
        }
        case "skills":
          return out(text(ask("skill", {})));
        case "skill": {
          const [name, file] = clean(args.slice(1));
          if (!name) return out("", 2, "raea skill NAME [FILE]  (raea skills lists them)\n");
          return out(text(ask("skill", file ? { name, file } : { name })));
        }
        case "craft": {
          const name = clean(args.slice(1))[0];
          return out(text(ask("craft", name ? { name } : {})));
        }
        default:
          return out(HELP, 2, `raea: no command "${args[0]}"\n`);
      }
    } catch (e) {
      return out("", 1, `raea ${args[0] ?? ""}: ${String((e as Error)?.message ?? e)}\n`);
    }
  };
}
