// `raea show`: what a session shows the person goes to the raea app, as an item they can use, not to Preview (Shane
// 11-09: "when using raea and raea app is installed new edits new sheets or examples or questions are pulled up in the
// raea app and not preview. we should see them as interactive too. the whole point of raea is interactive artifacts and
// things and recording them after making an answer"). The app's host makes the item (apps/app host/show.ts): stills a
// look (several, a storyboard), a video a watch, a sound a listen, a NAME.script.ts or .md a read, a .tsx an interactive
// artifact (or, when it's one of the project's compositions, that composition open in its editor), a question an ask.
// It lands quietly: the page finds it and says so in a notice, the window left where the person is (Shane 11-10: "any
// time you make an update to the feed it takes me out of the thing im currently doing"); --open brings the window up on
// it (raea://item/...). Several questions are one item, each question and each choice with a preview to play or read
// while answering (Shane 11-10: "all in one feed item rather than a bunch of them"). What the person sends on it comes
// back as lines in the session's inbox (~/.raea/inbox/<inbox>.jsonl). Without the app on this Mac, the files open in
// the Mac's own viewer and it says so. A render that still holds an error isn't shown (held.ts, SG15).
import { askWith } from "./held";

type Ask = (op: string, args: Record<string, unknown>) => any;

export const SHOW_USAGE = `raea show FILE... [--title T] [--ask "what you want from the person"] [--project P] [--replaces ITEM] [--open]
raea show --question "Which look?" (--choice A --choice B ... | STILL.png STILL.png ...) [--project P] [--open]
raea show [--title T] --question Q1 [--preview FILE] --choice A [--preview FILE] --choice B ... --question Q2 ... [--open]
  Ask everything at once: one item, every question in it, each question and each choice with what it is to play or
  read while answering (--preview after a --choice is that choice's, before any choice the question's: a still, a
  video, a sound, a NAME.script.ts or a .md). The first choice is the one you suggest.
  It lands in the person's feed with a notice and the window stays where they are; --open brings the window up on it
  (only when they're waiting on it).
  FILE: stills (several make a storyboard), a video, a sound, a NAME.script.ts or .md, or a .tsx (an interactive
  artifact; one of the project's compositions opens in its editor). P: Workspace/id (ids as raea workspace --json
  lists them), a folder in the project, or a Copy reference from the app; a title only when it's the only one and
  this folder isn't in another project. Else the one this folder is in. Who's showing it: --as NAME [--role ROLE]
  [--inbox NAME], else your harness's.
raea show FILE... --despite CHECK "why"
  A render that still holds an error (raea findings --open lists them) isn't shown: fix it and render again, or say
  why it goes anyway, once per check; the person reads the why on the item. One let through at the render (raea render
  --despite) needs none.
raea show FILE... --label "PLACE TAG" [--detail "PARAGRAPH"] --label ... | --labels LABELS.json
  Labels say what's where in what you show: each tag stands beside the picture, never on it, and points at its place
  when the person's pointer is on it. PLACE is x0,y0,x1,y1 or x,y in the picture's own 0-1, then #N for the Nth file
  shown (from 1), @T or @T-TO for a film's seconds, or part=KEY for a scene's part (its source key, or its name),
  joined without spaces (0,0,.5,1#2@3-5); the tag is the rest, a few words. A --detail is the paragraph its --label
  opens, where a tag isn't enough. --labels gives them as feed_post takes them. A sheet raea media sheet wrote comes
  labelled by itself (its OUT.labels.json, beside it). When one picture holds several things (a sheet, a side by side,
  a grid), label each instead of naming them by position in the title.
raea show --on ITEM --working ID... [--ask TEXT]... [--status S [--states S1,S2...]] | --done ID... [--note LINE] [--sub NAME]
raea show --on ITEM --working ID --sign N --clip FILE --render HASH
raea show FILE... --replaces ITEM --done ID...
  The person's edit comments are tasks they watch: mark each working as you start it and done when it's settled
  (the version that settles them, with --replaces); --sub names a subagent when several split them. With --working,
  each --ask is one ask you read the comment as, shown on it for the person to correct, --status where the note
  stands on its badge and --states each ask's (to restate, to check, failing, to sign, passing, signed); --sign shows
  ask N's clip, cut from the render whose sha256 is HASH, for the person to sign. A comment read as asks is a note: it
  closes when the person retires it, never with --done. raea notes post sends all of these from the notes.
`;

const WITH_VALUE = new Set(["--despite", "--label", "--detail", "--labels", "--title", "--ask", "--project", "--replaces", "--as", "--role", "--inbox", "--question", "--choice", "--preview", "--on", "--working", "--done", "--note", "--sub", "--status", "--states", "--sign", "--clip", "--render"]);
const FLAGS = new Set(["--open"]);

/** The harness an agent runs in, named as the feed shows it, from the program that started this command (native.ts
 *  walks up its parents): any harness, not only the ones that mark their shells. */
/** Whether a path is absolute: POSIX's /..., and Windows' C:\... (or C:/...), \\server\share\... and \... Until 0.1.8
 *  only a POSIX one was, so on Windows `raea show C:\...\cut.mp4` named a file under the cwd (#169). Here, not node:path:
 *  index.ts runs in the app's own shell too, with no Node. */
export const absolute = (f: string) => /^(?:[\\/]|[A-Za-z]:[\\/])/.test(f);

/** `f` from `cwd`: itself when it's absolute */
export const fromCwd = (cwd: string, f: string) => (absolute(f) ? f : `${cwd.replace(/[\\/]$/, "")}/${f}`);

export const HARNESSES: [RegExp, string][] = [
  [/^claude$/, "Claude Code"], [/^codex/, "Codex"], [/^gemini$/, "Gemini"], [/^kimi$/, "Kimi"], [/^opencode$/, "OpenCode"],
  [/^goose$/, "Goose"], [/^cursor-agent$/, "Cursor"], [/^amp$/, "Amp"], [/^qwen$/, "Qwen Code"], [/^crush$/, "Crush"], [/^aider$/, "Aider"],
];

/** The session's name and inbox, from its harness when it doesn't say: its shell's marks, else the harness that started
 *  this command (`parent`, any harness), one inbox per session so its answers find it. */
export function posterOf(env: Record<string, string | undefined>, as?: string, role?: string, inbox?: string, cwd = "", parent?: { name: string; pid: number }) {
  const marked = env.CLAUDECODE ? "Claude Code" : env.CODEX_SANDBOX || env.CODEX_HOME || env.CODEX_THREAD_ID ? "Codex" : env.GEMINI_CLI ? "Gemini" : env.OPENCODE ? "OpenCode" : undefined;
  const name = as?.trim() || marked || parent?.name || env.RAEA_AGENT || "An agent";
  const session = env.CLAUDE_CODE_SESSION_ID || env.CODEX_THREAD_ID || env.CODEX_SESSION_ID || env.RAEA_SESSION || (parent ? `p${parent.pid}` : hash(cwd));
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "agent";
  return { name, ...(role?.trim() ? { role: role.trim() } : {}), inbox: inbox?.trim() || `${slug}-${session.replace(/[^\w.-]/g, "").slice(0, 8)}` };
}

const hash = (s: string) => {
  let h = 5381;
  for (const c of s) h = ((h << 5) + h + c.charCodeAt(0)) >>> 0;
  return h.toString(36);
};

export function parseShow(args: string[], cwd: string) {
  const one = (name: string) => {
    const i = args.lastIndexOf(name);
    return i >= 0 ? args[i + 1] : undefined;
  };
  const many = (name: string) => args.flatMap((a, i) => (a === name && args[i + 1] !== undefined ? [args[i + 1]] : []));
  const choices = many("--choice");
  const unknown = args.find((a, i) => a.startsWith("--") && !WITH_VALUE.has(a) && !FLAGS.has(a) && !WITH_VALUE.has(args[i - 1] ?? ""));
  if (unknown) throw new Error(`no ${unknown} here\n${SHOW_USAGE}`);
  const abs = (f: string) => fromCwd(cwd, f);
  // --despite takes two: the check, then why
  const files = args.filter((a, i) => !a.startsWith("--") && !WITH_VALUE.has(args[i - 1] ?? "") && args[i - 2] !== "--despite").map(abs);
  const despite = args.flatMap((a, i) => {
    if (a !== "--despite") return [];
    const [check, why] = [args[i + 1], args[i + 2]];
    if (!check || !why || check.startsWith("--") || why.startsWith("--")) throw new Error(`--despite takes the check and why it goes anyway: --despite type-small "a sign seen in passing"\n${SHOW_USAGE}`);
    return [{ check, why }];
  });
  // the questions in order: each --question starts one and its --choice's follow; a --preview after a --choice is that
  // choice's, after the --question (before any choice) the question's own
  const asked: { ask: string; choices: { label: string; preview?: string }[]; preview?: string }[] = [];
  // the labels in order (docs/callouts.feature CO2b): each --label a place and its tag, a --detail its paragraph
  const labels: { place: string; text: string; detail?: string }[] = [];
  for (let i = 0; i < args.length; i++) {
    const v = args[i + 1];
    if (args[i] === "--label" && v !== undefined) {
      const m = /^\s*(\S+)\s+(\S[\s\S]*)$/.exec(v);
      if (!m) throw new Error(`--label "${v}" is a place, then its tag: --label "0,0,.5,1 v1, hook first"\n${SHOW_USAGE}`);
      labels.push({ place: m[1], text: m[2].trim() });
    } else if (args[i] === "--detail" && v !== undefined) {
      const l = labels.at(-1);
      if (!l) throw new Error(`--detail goes after the --label whose paragraph it is\n${SHOW_USAGE}`);
      if (l.detail !== undefined) throw new Error(`one --detail for the label "${l.text}"\n${SHOW_USAGE}`);
      l.detail = v;
    } else if (args[i] === "--question" && v !== undefined) asked.push({ ask: v, choices: [] });
    else if (args[i] === "--choice" && v !== undefined) asked.at(-1)?.choices.push({ label: v });
    else if (args[i] === "--preview" && v !== undefined) {
      const q = asked.at(-1);
      if (!q) throw new Error(`--preview goes after the --question or the --choice it shows\n${SHOW_USAGE}`);
      const at = q.choices.at(-1) ?? q;
      if (at.preview) throw new Error(`one --preview for ${"label" in at ? `the choice "${at.label}"` : `"${q.ask}"`}\n${SHOW_USAGE}`);
      at.preview = abs(v);
    }
  }
  if (args.includes("--choice") && args.indexOf("--choice") < args.indexOf("--question")) throw new Error(`a --choice goes after its --question\n${SHOW_USAGE}`);
  // several questions, or any preview: one item with them all (host/show.ts questions); else the one question as before
  const questions = asked.length > 1 || asked.some((q) => q.preview || q.choices.some((c) => c.preview)) ? asked : undefined;
  if ((labels.length || args.includes("--labels")) && (!files.length || questions || asked.length)) throw new Error(`labels go on the files you show: raea show FILE --label "PLACE TAG"\n${SHOW_USAGE}`);
  if (labels.length && args.includes("--labels")) throw new Error(`--label or --labels, not both\n${SHOW_USAGE}`);
  if (questions && files.length) throw new Error(`with several questions or previews, each file is a --preview of the question or choice it shows\n${SHOW_USAGE}`);
  return { labels, labelsFile: one("--labels"), files, despite, choices, questions, open: args.includes("--open"), title: one("--title"), ask: one("--ask"), asks: many("--ask"), project: one("--project"), replaces: one("--replaces"), question: questions ? undefined : one("--question"), as: one("--as"), role: one("--role"), inbox: one("--inbox"), on: one("--on"), working: many("--working"), done: many("--done"), note: one("--note"), sub: one("--sub"), status: one("--status"), states: one("--states")?.split(",").map((s) => s.trim()).filter(Boolean), sign: one("--sign"), clip: one("--clip"), render: one("--render") };
}

/** A --labels file's labels, as feed_post takes them: a list, or { "labels": [...] } (raea media sheet's OUT.labels.json). */
export function labelsIn(text: string, name: string): unknown[] {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (e) {
    throw new Error(`--labels ${name} isn't JSON: ${(e as Error).message}`);
  }
  const list = Array.isArray(json) ? json : (json as { labels?: unknown } | null)?.labels;
  if (!Array.isArray(list)) throw new Error(`--labels ${name} is a list of labels, or { "labels": [...] }, each { text, detail?, box? | point? | part?, frame?, t?, to? }`);
  return list;
}

/** `listed`: the --labels file's labels, read by the caller (labelsIn), which has the files. */
export function showVerb(ask: Ask, args: string[], cwd: string, env: Record<string, string | undefined>, listed?: unknown[]): string {
  if (!args.length || args[0] === "--help") return SHOW_USAGE;
  const a = parseShow(args, cwd);
  const parent = (): { name: string; pid: number } | undefined => {
    try {
      return ask("harness", {});
    } catch {
      return undefined; // a shell without the native raea: the show below says so
    }
  };
  const by = posterOf(env, a.as, a.role, a.inbox, cwd, a.as && a.inbox ? undefined : parent());
  // the person's edit comments as tasks (docs/review.feature R3): marked working or done on their item, or done by
  // the version shown with --replaces; a working mark carries the asks the agent read the comments as (DR5), for the
  // person to correct
  // where a note stands (DR6): its badge and each ask's, and a clip under one of its asks for the person to sign
  const noted = a.status !== undefined || a.states !== undefined || a.sign !== undefined;
  if (noted && !a.working.length) throw new Error(`--status, --states and --sign go with --working: raea show --on ITEM --working ID --status S\n${SHOW_USAGE}`);
  if (a.sign !== undefined && (a.working.length !== 1 || a.done.length || !a.clip || !a.render)) throw new Error(`--sign N shows one comment's clip: raea show --on ITEM --working ID --sign N --clip FILE --render HASH\n${SHOW_USAGE}`);
  if ((a.clip || a.render) && a.sign === undefined) throw new Error(`--clip and --render go with --sign N, the ask the clip is for\n${SHOW_USAGE}`);
  if (a.states && a.asks.length && a.states.length !== a.asks.length) throw new Error(`--states says one state for each --ask: ${a.asks.length} asks, ${a.states.length} states\n${SHOW_USAGE}`);
  const sign = a.sign === undefined ? undefined : { ask: Number(a.sign), clip: fromCwd(cwd, a.clip!), render: a.render };
  const mark = (item: string, comments: string[], state: "working" | "done", version?: string) =>
    ask("task", { item, comments, state, by, sub: a.sub, note: state === "done" ? a.note : undefined, asks: state === "working" && a.asks.length ? a.asks : undefined, version, ...(state === "working" ? { status: a.status, states: a.states, sign } : {}) }) as { item: string; comments: string[] };
  const said = (m: { item: string; comments: string[] }, state: string) => `Marked ${m.comments.join(", ")} ${state} on ${m.item}: the person sees ${a.sub ? `${by.name} · ${a.sub}` : by.name} ${state === "working" ? "on them" : "finished them"}${state === "working" && a.status ? `; it reads "${a.status}"${a.states?.length ? ` (its asks: ${a.states.join(", ")})` : ""}` : ""}.`;
  if (!a.files.length && !a.question && !a.questions && (a.working.length || a.done.length)) {
    const item = a.on ?? a.replaces;
    if (!item) throw new Error(`which item the comments are on: --on ITEM\n${SHOW_USAGE}`);
    if (sign) {
      const m = mark(item, a.working, "working");
      return `Shown ask ${sign.ask}'s clip on ${m.comments[0]} of ${m.item} for the person to sign (render ${String(sign.render).slice(0, 12)}…): their sign comes to the inbox, and raea notes take keeps it.\n`;
    }
    const out = [...(a.working.length ? [said(mark(item, a.working, "working"), "working")] : []), ...(a.done.length ? [said(mark(item, a.done, "done"), "done")] : [])];
    return `${out.join("\n")}\n`;
  }
  if (a.working.length) throw new Error(`--working marks comments without showing anything: raea show --on ITEM --working ID\n${SHOW_USAGE}`);
  if (a.done.length && !a.replaces) throw new Error(`--done with files settles comments on the item they replace: add --replaces ITEM\n${SHOW_USAGE}`);
  if (a.labelsFile !== undefined && !listed) throw new Error(`--labels ${a.labelsFile}: its labels weren't read (raea's own handler reads the file)`);
  const labels = a.labels.length ? a.labels : listed;
  const r = ask("show", { ...(labels?.length ? { labels } : {}), files: a.files, despite: a.despite, title: a.title, ask: askWith(a.ask, a.despite), question: a.question, choices: a.choices, questions: a.questions, open: a.open, project: a.project, replaces: a.replaces, cwd, by }) as
    | { opened: string[] }
    | { kind: "item" | "composition"; id: string; type: string; title: string; project: string; workspace: string; inbox: string; brought?: boolean };
  if ("opened" in r) return `The raea app isn't on this Mac, so ${r.opened.length === 1 ? "it opened" : `${r.opened.length} files opened`} in the Mac's own viewer: nothing was posted and nothing comes back. Install raea (raea.io) to show work as items the person can answer.\n`;
  const home = env.HOME ?? "";
  const inbox = home && r.inbox.startsWith(home) ? `~${r.inbox.slice(home.length)}` : r.inbox;
  if (r.kind === "composition")
    return r.brought
      ? `Opened ${r.title} in the raea app's composition editor (project ${r.project}, ${r.workspace}): the person plays it and changes it there, and their changes save into its file.\n`
      : `Offered ${r.title} in the raea app (project ${r.project}, ${r.workspace}): a notice opens it in the composition editor when the person is ready; --open brings the window up on it now.\n`;
  // the version is posted by now: a marking the host refuses is said with it, not thrown over it
  const settle = (item: string) => {
    try {
      return said(mark(item, a.done, "done", r.id), "done");
    } catch (e) {
      return `Not marked done: ${(e as Error).message.replace(/^raea show: /, "")} (mark them with raea show --on ${item} --done ID).`;
    }
  };
  const settled = a.done.length && a.replaces ? [settle(a.replaces)] : [];
  return [
    r.brought
      ? `Shown in the raea app: "${r.title}" (${r.type}, item ${r.id}, project ${r.project} in ${r.workspace}); the window is on it.`
      : `Posted in the raea app: "${r.title}" (${r.type}, item ${r.id}, project ${r.project} in ${r.workspace}), with a notice; the window stayed where the person is.`,
    ...settled,
    `What the person sends on it (notes, edits, picks, answers, results) comes to ${inbox}, one JSON line each. Follow it (tail -n0 -F ${inbox}, or your harness's monitor) and answer each; a next version is raea show --replaces ${r.id} FILE.`,
    "",
  ].join("\n");
}
