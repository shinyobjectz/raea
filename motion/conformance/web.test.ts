// The web globals bundled packages expect (raea-native's shims in the engine; bun's own under plain bun): the same
// answers in both, so a package behaves the same on the Mac, the iPhone and in tests.
import { describe, expect, test } from "bun:test";

describe("web globals", () => {
  test("TextEncoder / TextDecoder: UTF-8 both ways, streaming, replacement, fatal", () => {
    const te = new TextEncoder(), td = new TextDecoder();
    const s = "héllo 😀 世界";
    expect(td.decode(te.encode(s))).toBe(s);
    expect(Array.from(te.encode("é😀"))).toEqual([195, 169, 240, 159, 152, 128]);
    const parts = te.encode("a😀b");
    const d = new TextDecoder();
    expect(d.decode(parts.slice(0, 3), { stream: true }) + d.decode(parts.slice(3))).toBe("a😀b");
    expect(td.decode(new Uint8Array([0x61, 0xff, 0x62]))).toBe("a�b");
    expect(() => new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff]))).toThrow(TypeError);
    expect(new TextDecoder().decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x41]))).toBe("A");
    const into = new Uint8Array(4);
    expect(te.encodeInto("aé", into)).toEqual({ read: 2, written: 3 });
  });
  test("URL and URLSearchParams by the WHATWG rules", () => {
    const u = new URL("../c?x=1&y=two words#h", "https://a.example:8080/b/d");
    expect(u.href).toBe("https://a.example:8080/c?x=1&y=two%20words#h");
    expect([u.protocol, u.hostname, u.port, u.pathname, u.search, u.hash, u.origin]).toEqual(["https:", "a.example", "8080", "/c", "?x=1&y=two%20words", "#h", "https://a.example:8080"]);
    expect(u.searchParams.get("y")).toBe("two words");
    u.searchParams.set("x", "2");
    expect(u.search).toBe("?x=2&y=two+words");
    u.pathname = "/z";
    expect(u.pathname).toBe("/z");
    expect(() => new URL("nope")).toThrow(TypeError);
    expect(URL.canParse("file:///x")).toBe(true);
    expect(new URL("HTTP://EXAMPLE.com/a/../b").href).toBe("http://example.com/b");
    const p = new URLSearchParams({ a: "1", b: "x y" });
    p.append("a", "2");
    expect(p.getAll("a")).toEqual(["1", "2"]);
    expect(p.toString()).toBe("a=1&b=x+y&a=2");
  });
  test("AbortController, EventTarget", () => {
    const ac = new AbortController();
    let fired = 0;
    ac.signal.addEventListener("abort", () => fired++);
    ac.abort();
    ac.abort();
    expect(ac.signal.aborted).toBe(true);
    expect(fired).toBe(1);
    expect((ac.signal.reason as Error).name).toBe("AbortError");
    expect(() => ac.signal.throwIfAborted()).toThrow();
    const t = new EventTarget();
    let n = 0;
    t.addEventListener("x", () => n++, { once: true });
    t.dispatchEvent(new Event("x"));
    t.dispatchEvent(new Event("x"));
    expect(n).toBe(1);
  });
  test("atob, btoa, structuredClone", () => {
    expect(btoa("abc")).toBe("YWJj");
    expect(atob(btoa("hiÿ"))).toBe("hiÿ");
    expect(() => btoa("😀")).toThrow();
    const o: Record<string, unknown> = { a: [1, { b: new Date(5) }], m: new Map([[1, 2]]), s: new Set([3]), u: new Uint8Array([1, 2]) };
    o.self = o;
    const c = structuredClone(o) as typeof o;
    expect(c).not.toBe(o);
    expect(c.self).toBe(c);
    expect(((c.a as unknown[])[1] as { b: Date }).b.getTime()).toBe(5);
    expect((c.m as Map<number, number>).get(1)).toBe(2);
    expect(Array.from(c.u as Uint8Array)).toEqual([1, 2]);
    expect(() => structuredClone(() => 1)).toThrow();
  });
  test("crypto: random values, UUIDs, SHA digests", async () => {
    expect(crypto.getRandomValues(new Uint8Array(32)).some((x) => x !== 0)).toBe(true);
    expect(crypto.randomUUID()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    const hex = (b: ArrayBuffer) => Array.from(new Uint8Array(b), (x) => x.toString(16).padStart(2, "0")).join("");
    const abc = new TextEncoder().encode("abc");
    expect(hex(await crypto.subtle.digest("SHA-256", abc))).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    expect(hex(await crypto.subtle.digest("SHA-1", abc))).toBe("a9993e364706816aba3e25717850c26c9cd0d89d");
  });
  test("Blob and timers", async () => {
    const b = new Blob(["ab", new Uint8Array([99])], { type: "text/plain" });
    expect(b.size).toBe(3);
    expect(await b.text()).toBe("abc");
    expect(await b.slice(1).text()).toBe("bc");
    const t0 = Date.now();
    await new Promise((r) => setTimeout(r, 30));
    expect(Date.now() - t0).toBeGreaterThanOrEqual(25);
  });
});
