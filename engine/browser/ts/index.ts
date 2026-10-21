// @raea/browser: a browser the agent drives, typed. Each call is agent-browser's command line words run in process
// (raea-browser, `browser.run`): Chrome over CDP on the Mac. Snapshots name elements by ref (e1, e2, ...), and the
// other verbs take a ref ("@e2") or a selector. Every call answers the daemon's response; `text` is what the command
// line would print, for the agent to read.
import { native } from "@raea/motion/native";

export interface BrowserResult<T = unknown> {
  success: boolean;
  data: T;
  error: string | null;
  code: string | null;
  text: string;
}
export interface SnapshotOptions {
  /** interactive elements only (buttons, inputs, links) */
  interactive?: boolean;
  /** drop empty structural elements */
  compact?: boolean;
  depth?: number;
  /** scope to a CSS selector */
  selector?: string;
  urls?: boolean;
}
export interface ScreenshotOptions {
  full?: boolean;
  /** numbered labels on interactive elements ([N] is @eN) */
  annotate?: boolean;
  format?: "png" | "jpeg";
  quality?: number;
}

function run<T = unknown>(args: (string | number | undefined | false)[]): BrowserResult<T> {
  const words = args.filter((a): a is string | number => a !== undefined && a !== false).map(String);
  const r = native("browser.run" as never, { args: words } as never) as BrowserResult<T>;
  return r;
}
function must<T>(r: BrowserResult<T>): BrowserResult<T> {
  if (!r.success) throw new Error(r.error ?? r.text.trim() ?? "the browser refused");
  return r;
}

/** The browser (one per process, as agent-browser's daemon keeps one; `session` names an isolated one). */
export class Browser {
  constructor(readonly session?: string) {}
  private words(...a: (string | number | undefined | false)[]) {
    return this.session ? ["--session", this.session, ...a] : a;
  }
  /** Any command line words, as `agent-browser WORDS...` takes them; the result unchecked. */
  raw(...words: string[]): BrowserResult {
    return run(this.words(...words));
  }
  open(url?: string) {
    return must(run(this.words("open", url)));
  }
  /** The accessibility tree with refs; `refs` maps each ref to its role and name. */
  snapshot(o: SnapshotOptions = {}) {
    return must(run<{ snapshot: string; refs: Record<string, { role: string; name?: string }> }>(this.words("snapshot", o.interactive && "-i", o.compact && "-c", o.depth !== undefined && "-d", o.depth, o.selector && "-s", o.selector, o.urls && "--urls")));
  }
  click(target: string) {
    return must(run(this.words("click", target)));
  }
  fill(target: string, text: string) {
    return must(run(this.words("fill", target, text)));
  }
  type(target: string, text: string) {
    return must(run(this.words("type", target, text)));
  }
  press(key: string) {
    return must(run(this.words("press", key)));
  }
  hover(target: string) {
    return must(run(this.words("hover", target)));
  }
  /** text, html, value, title, url, count, box, or an attribute (`get("attr", "@e1", "href")`) */
  get(what: string, target?: string, extra?: string) {
    return must(run(this.words("get", what, target, extra)));
  }
  screenshot(path?: string, o: ScreenshotOptions = {}) {
    return must(run(this.words("screenshot", path, o.full && "--full", o.annotate && "--annotate", o.format && "--screenshot-format", o.format, o.quality !== undefined && "--screenshot-quality", o.quality)));
  }
  eval(js: string) {
    return must(run(this.words("eval", js)));
  }
  /** diff snapshot | screenshot --baseline FILE | url A B */
  diff(...words: string[]) {
    return run(this.words("diff", ...words));
  }
  /** several commands in one call, each its own words */
  batch(commands: string[][], bail = false) {
    return run(this.words("batch", bail && "--bail", ...commands.map((c) => c.map((w) => (/[\s"']/.test(w) ? JSON.stringify(w) : w)).join(" "))));
  }
  wait(...words: string[]) {
    return must(run(this.words("wait", ...words)));
  }
  close() {
    return run(this.words("close"));
  }
}

export const browser = new Browser();
