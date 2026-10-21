// @raea/browser on the Mac (Chrome over CDP, agent-browser in process): a fixture form opened, read as a snapshot with
// refs, filled and clicked by ref, and the result read back; a screenshot written. Skipped where there's no browser.
import { describe, expect, test } from "bun:test";
import { hasNative, native } from "@raea/motion/native";
import { Browser } from "../ts";

const root = engineRoot();
function engineRoot(): string | null {
  if (!hasNative()) return null;
  return native("host.env", { name: "RAEA_ROOT" }) as string | null;
}
const hasBrowser = (() => {
  try {
    return hasNative() && native("host.platform", {}) === "macos" && (native("host.env", { name: "RAEA_NO_BROWSER" }) as string | null) !== "1" && !!root;
  } catch {
    return false;
  }
})();

describe.skipIf(!hasBrowser)("the browser (Chrome over CDP, in process)", () => {
  const b = new Browser(`raea-test-${Date.now()}`);
  test("open, snapshot with refs, fill and click by ref, read the result", () => {
    b.open(`file://${root}/engine/browser/conformance/pages/form.html`);
    const snap = b.snapshot({ interactive: true });
    const refs = snap.data.refs;
    const find = (role: string, name: string) => Object.entries(refs).find(([, r]) => r.role === role && (r.name ?? "").includes(name))?.[0];
    const email = find("textbox", "Email");
    const join = find("button", "Join");
    expect(email).toBeDefined();
    expect(join).toBeDefined();
    expect(snap.data.snapshot).toContain(`[ref=${email}]`);
    b.fill(`@${email}`, "kelly@example.com");
    b.click(`@${join}`);
    expect(String(b.get("text", "#out").data && (b.get("text", "#out").data as { text?: string }).text)).toBe("Thanks, kelly@example.com on Go");
    expect((b.get("title").data as { title?: string }).title).toBe("Sign up");
  }, 60000);
  test("eval, a screenshot to a file, batch, and a refusal", () => {
    expect((b.eval("1 + 2").data as { result?: unknown }).result).toBe(3);
    const tmp = (native("host.env", { name: "TMPDIR" }) as string) + `raea-browser-${Date.now()}.png`;
    b.screenshot(tmp);
    expect(native("media.size", { path: tmp })).toBeGreaterThan(1000);
    const r = b.batch([["get", "title"], ["get", "url"]]);
    expect(r.success).toBe(true);
    expect(() => b.click("#nothing-here")).toThrow();
    b.close();
  }, 60000);
});

describe.skipIf(!hasBrowser)("the browser as a shell command", () => {
  test("browser open, snapshot and screenshot into the project", async () => {
    const { Shell } = await import("@raea/shell");
    const tmp = native("host.env", { name: "TMPDIR" }) as string;
    const sh = new Shell({ root: `${tmp}raea-browser-shell-${Date.now()}` });
    await sh.exec(`browser --session shelltest open file://${root}/engine/browser/conformance/pages/form.html`);
    const snap = await sh.exec("browser --session shelltest snapshot -i");
    expect(snap.exitCode).toBe(0);
    expect(snap.stdout).toContain("textbox");
    const shot = await sh.exec("browser --session shelltest screenshot shots/form.png && ls shots");
    expect(shot.stdout).toBe("✓ Screenshot saved to /shots/form.png\nform.png\n");
    expect((shot.row as any).written.map((w: any) => w.path)).toContain("/shots/form.png");
    await sh.exec("browser --session shelltest close");
  }, 60000);
  // the shell is offline, and so are its pages: a page's fetch to the web is refused (2026-10-18, void's agent pulled a
  // model from raw.githubusercontent.com this way), while a project page (file:) still opens
  test("a page can't reach the network; a project page still opens", async () => {
    const { Shell } = await import("@raea/shell");
    const tmp = native("host.env", { name: "TMPDIR" }) as string;
    const sh = new Shell({ root: `${tmp}raea-browser-offline-${Date.now()}` });
    const opened = await sh.exec(`browser --session offlinetest open file://${root}/engine/browser/conformance/pages/form.html`);
    expect(opened.exitCode).toBe(0);
    const url = "https://raw.githubusercontent.com/mrdoob/three.js/dev/README.md";
    const r = await sh.exec(`browser --session offlinetest eval "(async()=>{try{const r=await fetch('${url}');return 'reached '+r.status}catch(e){return 'refused'}})()"`);
    expect(r.stdout).toContain("refused");
    expect(r.stdout).not.toContain("reached");
    const ws = await sh.exec(`browser --session offlinetest eval "(()=>{try{new WebSocket('wss://echo.websocket.org');return 'opened'}catch(e){return 'refused'}})()"`);
    expect(ws.stdout).toContain("refused");
    await sh.exec("browser --session offlinetest close");
  }, 60000);
});

const blitzOnly = hasNative() && !!root;
describe.skipIf(!blitzOnly)("the Blitz backend (no Chrome)", () => {
  const b = new Browser(`raea-blitz-${Date.now()}`);
  const words = (...w: string[]) => b.raw("--engine", "blitz", ...w);
  test("open, snapshot, fill and click by ref, read the result, screenshot", () => {
    expect(words("open", `file://${root}/engine/browser/conformance/pages/form.html`).success).toBe(true);
    const snap = words("snapshot", "-i");
    expect(snap.success).toBe(true);
    const refs = (snap.data as { refs: Record<string, { role: string; name: string }> }).refs;
    const find = (role: string, name: string) => Object.entries(refs).find(([, r]) => r.role === role && r.name === name)?.[0];
    expect(words("fill", `@${find("textbox", "Email")}`, "kelly@example.com").success).toBe(true);
    expect(words("click", `@${find("button", "Join")}`).success).toBe(true);
    expect(((words("get", "text", "#out").data) as { text: string }).text).toBe("Thanks, kelly@example.com on Go");
    expect((words("get", "title").data as { title: string }).title).toBe("Sign up");
    expect((words("eval", "1 + 2").data as { result: unknown }).result).toBe(3);
    const tmp = (native("host.env", { name: "TMPDIR" }) as string) + `raea-blitz-${Date.now()}.png`;
    expect(words("screenshot", tmp).success).toBe(true);
    expect(native("media.size", { path: tmp })).toBeGreaterThan(1000);
    expect(words("click", "@e99").success).toBe(false);
    words("close");
  }, 60000);
});

describe.skipIf(!hasBrowser)("parity: the same refs from Chrome and from Blitz", () => {
  for (const page of ["form.html", "list.html"]) {
    test(`${page}: interactive snapshots agree line for line`, () => {
      const url = `file://${root}/engine/browser/conformance/pages${page}`;
      const chrome = new Browser(`parity-chrome-${Date.now()}`);
      chrome.open(url);
      const c = chrome.snapshot({ interactive: true }).text.trim();
      chrome.close();
      const blitz = new Browser(`parity-blitz-${Date.now()}`);
      blitz.raw("--engine", "blitz", "open", url);
      const z = blitz.raw("--engine", "blitz", "snapshot", "-i").text.trim();
      blitz.raw("--engine", "blitz", "close");
      expect(z).toBe(c);
    }, 60000);
  }
});
