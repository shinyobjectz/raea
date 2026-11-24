// bundle_script's entry (fresh.rs): the default export called and awaited; what it returns printed (a string as it
// is, anything else as JSON), and a throw printed as its name and message, then its stack, on stderr. A script's answer
// was dropped and a throw printed its stack alone, which on JavaScriptCore is only the frames, without the message (#168).
const show = (v: any): string => { if (typeof v === "string") return v; try { return JSON.stringify(v, null, 2) ?? String(v); } catch { return String(v); } };
const said = (e: any): string => { const head = e && typeof e === "object" && "message" in e ? (e.name || "Error") + ": " + e.message : String(e); const stack = e && e.stack ? String(e.stack) : ""; return stack.startsWith(head) ? stack : stack ? head + "\n" + stack : head; };
Promise.resolve().then(() => (typeof (M as any).default === "function" ? (M as any).default() : undefined)).then((v: any) => { if (v !== undefined) call("print", JSON.stringify({ text: show(v) })); call("done", JSON.stringify({ code: 0 })); }, (e: any) => { call("print", JSON.stringify({ text: "error: " + said(e), err: true })); call("done", JSON.stringify({ code: 1 })); });
