// The DOM-like surface (docs/ts-native.feature P18, src/dom.ts) through React's DOM mode on the stand-in host: refs
// are elements, the tree and attributes are mirrored, geometry comes from a layout snapshot, events go through capture
// and bubble with React's handlers in Chrome's order, and the observers report. The same behaviours are proved against
// Chrome by the parity cases (sdk/ui/parity/cases/p18-dom); this is the fast check.
import { beforeAll, describe, expect, test } from "bun:test";
import { useEffect, useRef, useState } from "react";
import { getComputedStyle, install, layout } from "../src/dom";
import { createPortal, flushSync, mount } from "../src/react";
import { flush } from "../src/wire";
import { fresh, snap } from "./host";
import "./dom-dashboard"; // its tests run here, after parity.test.tsx, which wants no DOM installed

beforeAll(() => install());
const g = globalThis as any;

const press = (x: number, y: number, detail = 1) => ({ x: 0, y: 0, clientX: x, clientY: y, button: 0, detail });

describe("P18 DOM surface", () => {
  test("a ref is an element: tag, attributes, tree, queries, instanceof", () => {
    const host = fresh();
    let ref: any;
    function App() {
      const r = useRef<any>(null);
      useEffect(() => void (ref = r.current), []);
      return (
        <div data-testid="root" id="app" className="a b">
          <ul ref={r} data-testid="list" aria-label="things" role="listbox">
            <li data-testid="x" className="item" data-k="1">
              one
            </li>
            <li data-testid="y" className="item sel" aria-selected={true}>
              two
            </li>
          </ul>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    expect(ref).toBeInstanceOf(g.HTMLElement);
    expect(ref).toBeInstanceOf(g.Element);
    expect(ref.tagName).toBe("UL");
    expect(ref.getAttribute("aria-label")).toBe("things");
    expect(ref.ownerDocument).toBe(g.document);
    expect(ref.ownerDocument.defaultView).toBe(g);
    expect(ref.isConnected).toBe(true);
    expect(ref.textContent).toBe("onetwo");
    expect(ref.children.length).toBe(2);
    expect(ref.querySelectorAll(".item").length).toBe(2);
    expect(ref.querySelector("[aria-selected=true]").getAttribute("data-testid")).toBe("y");
    expect(ref.querySelector("li:not(.sel)").dataset.k).toBe("1");
    expect(ref.firstElementChild.closest("#app").className).toBe("a b");
    expect(g.document.getElementById("app").contains(ref)).toBe(true);
    expect(g.document.body.contains(ref)).toBe(true);
    expect(g.document.querySelector("#root > #app ul > li + li").getAttribute("data-testid")).toBe("y");
    expect(ref.matches("ul[role=listbox]")).toBe(true);
    // setAttribute goes to the host as a prop
    ref.setAttribute("data-state", "open");
    g.__flushTest?.();
  });

  test("a box with its own z-index stays under the overflow clip of the box it's in; a fixed one doesn't", () => {
    const host = fresh();
    function App() {
      return (
        <div data-testid="root">
          <button data-testid="side" />
          <div data-testid="stage" style={{ overflow: "hidden" }}>
            <div data-testid="view" style={{ zIndex: 2 }}>
              <div data-testid="card" style={{ zIndex: 3, overflow: "hidden" }}>
                <img data-testid="pic" />
              </div>
            </div>
            <div data-testid="menu" style={{ position: "fixed", zIndex: 5 }} />
          </div>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    // the card is zoomed past the stage's edge, over the button beside the stage; the menu is placed there on purpose
    const z = (n: number) => [...Array(12).fill(0), n];
    const boxes: Record<string, number[]> = { root: [0, 0, 800, 600], side: [0, 0, 200, 600], stage: [200, 0, 600, 600], view: [200, 0, 1600, 1200, ...z(2)], card: [100, 100, 400, 300, ...z(3)], pic: [100, 100, 400, 300, ...z(3)], menu: [20, 500, 100, 40, ...z(5)] };
    const n: number[] = [];
    for (const [t, b] of Object.entries(boxes)) n.push(host.tid(t), ...b, ...Array(17 - b.length).fill(0));
    layout(JSON.stringify({ vw: 800, vh: 600, dpr: 2, n }));
    const at = (x: number, y: number) => g.document.elementFromPoint(x, y)?.getAttribute("data-testid");
    expect(at(150, 150)).toBe("side");
    // nor do the card's own children, though the card clips too: its clip is cut to the stage's, not put in its place
    expect(at(300, 150)).toBe("pic");
    expect(at(700, 500)).toBe("view");
    expect(at(50, 510)).toBe("menu");
  });

  test("a press goes where the host's cascade says: a child a stylesheet gives pointer events back, under a pointer-events-none layer", () => {
    const host = fresh();
    const log: string[] = [];
    function App() {
      return (
        <div data-testid="root" onClick={() => log.push("behind")}>
          <div data-testid="dock" className="pointer-events-none">
            <div data-testid="row" className="[&>*]:pointer-events-auto">
              <button data-testid="ok" onClick={() => log.push("allowed")}>Allow</button>
            </div>
          </div>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    snap(host, { root: [0, 0, 800, 600], dock: [0, 400, 800, 200], row: [0, 400, 800, 100], ok: [10, 410, 40, 24] });
    // without the host's target, sdk/ui's own walk can't read the child variant and the press falls through
    host.send("root", "pointerdown", press(20, 420));
    host.send("root", "pointerup", press(20, 420));
    expect(log).toEqual(["behind"]);
    log.length = 0;
    // with it (Blitz's hit path, decided by Stylo as Chrome would), the button gets the click
    const target = host.tid("ok");
    host.send("root", "pointerdown", { ...press(20, 420), target });
    host.send("root", "pointerup", { ...press(20, 420), target });
    expect(log).toEqual(["allowed", "behind"]);
  });

  test("a press: capture and bubble through native and React listeners in Chrome's order, then click", () => {
    const host = fresh();
    const log: string[] = [];
    let btn: any;
    function App() {
      useEffect(() => {
        const d = g.document;
        const cap = () => log.push("doc capture");
        const bub = () => log.push("doc bubble");
        d.addEventListener("pointerdown", cap, true);
        d.addEventListener("pointerdown", bub);
        btn.addEventListener("pointerdown", (e: any) => log.push(`native at ${e.currentTarget.tagName} phase ${e.eventPhase}`));
        g.addEventListener("click", () => log.push("window click"));
      }, []);
      return (
        <div data-testid="root" onPointerDownCapture={() => log.push("react capture root")} onPointerDown={() => log.push("react bubble root")} onClick={(e) => log.push(`react click ${(e.target as any).tagName}`)}>
          <button data-testid="b" ref={(e: any) => (btn = e)} onPointerDown={(e) => log.push(`react ${e.type} ${(e.currentTarget as any).tagName}`)}>
            <span data-testid="s">hi</span>
          </button>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    snap(host, { root: [0, 0, 800, 600], b: [10, 10, 100, 40], s: [20, 20, 30, 20] });
    host.send("b", "pointerdown", press(25, 25));
    host.send("b", "pointerup", press(25, 25));
    expect(log).toEqual([
      "doc capture",
      "react capture root",
      "native at BUTTON phase 3",
      "react pointerdown BUTTON",
      "react bubble root",
      "doc bubble",
      "react click SPAN",
      "window click",
    ]);
    // the press focused the button (natively focusable)
    expect(g.document.activeElement).toBe(btn);
  });

  test("stopPropagation in React stops native listeners above the root; preventDefault on mousedown keeps focus", () => {
    const host = fresh();
    const log: string[] = [];
    function App() {
      useEffect(() => {
        const f = () => log.push("doc");
        g.document.addEventListener("pointerdown", f);
        return () => g.document.removeEventListener("pointerdown", f);
      }, []);
      return (
        <div data-testid="root">
          <button data-testid="a">a</button>
          <button data-testid="b" onPointerDown={(e) => e.stopPropagation()} onMouseDown={(e) => e.preventDefault()}>
            b
          </button>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    snap(host, { root: [0, 0, 800, 600], a: [0, 0, 50, 20], b: [0, 30, 50, 20] });
    const a = g.document.querySelector("[data-testid=a]");
    a.focus();
    expect(g.document.activeElement).toBe(a);
    host.send("b", "pointerdown", press(10, 40));
    expect(log).toEqual([]);
    expect(g.document.activeElement).toBe(a);
  });

  test("focus(): blur, focusout, focus, focusin in order, activeElement synchronous, React's onFocus/onBlur bubble", () => {
    fresh();
    const log: string[] = [];
    let a: any;
    let b: any;
    mount(
      <div data-testid="root" onFocus={(e) => log.push(`react focus ${(e.target as any).id}`)} onBlur={(e) => log.push(`react blur ${(e.target as any).id}`)}>
        <input id="a" data-testid="a" ref={(e: any) => (a = e)} />
        <button id="b" data-testid="b" ref={(e: any) => (b = e)}>
          b
        </button>
      </div>,
      0,
      { dom: true },
    );
    for (const t of ["focus", "blur", "focusin", "focusout"]) g.document.addEventListener(t, (e: any) => log.push(`${t} ${e.target.id} ${g.document.activeElement?.id || "body"}`), true);
    a.focus();
    b.focus();
    expect(g.document.activeElement).toBe(b);
    expect(log).toEqual(["focus a a", "focusin a a", "react focus a", "blur a body", "focusout a body", "react blur a", "focus b b", "focusin b b", "react focus b"]);
  });

  test("a portal: in body in the DOM, React's events bubble to where it was rendered", () => {
    const host = fresh();
    const log: string[] = [];
    function App() {
      const [on] = useState(true);
      return (
        <div data-testid="root" onClick={() => log.push("root got the portal's click")}>
          {on && createPortal(<div data-testid="p">portal</div>, g.document.body)}
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    const p = g.document.querySelector("[data-testid=p]");
    expect(p.parentNode).toBe(g.document.body);
    expect(g.document.getElementById("root").contains(p)).toBe(false);
    snap(host, { root: [0, 0, 800, 600], p: [0, 0, 100, 100] });
    // the host's root hears presses (no element in the portal listens)
    host.send(0, "pointerdown", press(5, 5));
    host.send(0, "pointerup", press(5, 5));
    expect(log).toEqual(["root got the portal's click"]);
  });

  test("observers: ResizeObserver and IntersectionObserver from the snapshot, MutationObserver from the tree", async () => {
    const host = fresh();
    const log: string[] = [];
    let el: any;
    function App() {
      const [n, setN] = useState(1);
      (g as any).__setN = setN;
      return (
        <div data-testid="root">
          <div data-testid="box" ref={(e: any) => (el = e)}>
            {Array.from({ length: n }, (_, i) => (
              <span key={i}>{i}</span>
            ))}
          </div>
        </div>
      );
    }
    mount(<App />, 0, { dom: true });
    new g.ResizeObserver((es: any[]) => es.forEach((e) => log.push(`resize ${e.contentRect.width}x${e.contentRect.height}`))).observe(el);
    new g.IntersectionObserver((es: any[]) => es.forEach((e) => log.push(`intersect ${e.isIntersecting} ${e.intersectionRatio}`))).observe(el);
    new g.MutationObserver((rs: any[]) => rs.forEach((r) => log.push(`mutation ${r.type} +${r.addedNodes.length}`))).observe(el, { childList: true });
    snap(host, { root: [0, 0, 800, 600], box: [0, 550, 100, 100] });
    expect(log).toEqual(["resize 100x100", "intersect true 0.5"]);
    snap(host, { root: [0, 0, 800, 600], box: [0, 550, 100, 100] });
    expect(log.length).toBe(2);
    flushSync(() => g.__setN(2));
    await Promise.resolve();
    await Promise.resolve();
    expect(log[2]).toBe("mutation childList +1");
    snap(host, { root: [0, 0, 800, 600], box: [0, 700, 120, 100] });
    expect(log.slice(3)).toEqual(["resize 120x100", "intersect false 0"]);
  });

  test("document.createElement goes onto the host once connected; matchMedia; getComputedStyle; TreeWalker", () => {
    const host = fresh();
    mount(<div data-testid="root" />, 0, { dom: true });
    const d = g.document.createElement("div");
    d.setAttribute("data-testid", "made");
    d.style.setProperty("width", "40px");
    expect(() => host.tid("made")).toThrow();
    g.document.body.appendChild(d);
    g.__ui_flush; // ops go at the next microtask
    expect(d.isConnected).toBe(true);
    expect(g.getComputedStyle(d).display).toBe("block");
    expect(g.getComputedStyle(d).getPropertyValue("animation-name")).toBe("none");
    expect(g.matchMedia("(min-width: 640px)").matches).toBe(true);
    expect(g.matchMedia("(prefers-reduced-motion: reduce)").matches).toBe(false);
    const w = g.document.createTreeWalker(g.document.body, g.NodeFilter.SHOW_ELEMENT, { acceptNode: (n: any) => (n.tagName === "DIV" ? 1 : 3) });
    const seen: string[] = [];
    for (let n = w.nextNode(); n; n = w.nextNode()) seen.push(n.getAttribute("data-testid") ?? n.id);
    expect(seen).toEqual(["root", "root", "made"]);
  });
});

describe("dangerouslySetInnerHTML", () => {
  test("its markup becomes the element's children on the host, an inline SVG in SVG's namespace, and follows a change", () => {
    const host = fresh();
    const icon = (fill: string) => `<svg viewBox="0 0 24 24" width="1em" height="1em"><title>Mark</title><path fill="${fill}" d="M0 0h24v24H0z"></path></svg>`;
    let set!: (s: string) => void;
    let ref: any;
    function App() {
      const [fill, setFill] = useState("#D97757");
      set = setFill;
      const r = useRef<any>(null);
      useEffect(() => void (ref = r.current), []);
      return <span ref={r} data-testid="mark" dangerouslySetInnerHTML={{ __html: icon(fill) }} />;
    }
    mount(<App />, 0, { dom: true });
    const span = host.nodes.get(host.tid("mark"))!;
    const svg = host.nodes.get(span.children[0])!;
    expect(svg.kind).toBe("svg");
    expect(svg.props.viewBox).toBe("0 0 24 24");
    const path = svg.children.map((c) => host.nodes.get(c)!).find((n) => n.kind === "path")!;
    expect(path.props.fill).toBe("#D97757");
    expect(ref.querySelector("path").namespaceURI).toBe("http://www.w3.org/2000/svg");
    flushSync(() => set("#1783FF"));
    const svg2 = host.nodes.get(host.nodes.get(host.tid("mark"))!.children[0])!;
    expect(svg2.children.map((c) => host.nodes.get(c)!).find((n) => n.kind === "path")!.props.fill).toBe("#1783FF");
    expect(host.nodes.get(host.tid("mark"))!.children.length).toBe(1);
  });
});

describe("video and audio play on the host's player (runtime/dom media.rs, raea-runtime-js media_play.rs)", () => {
  test("play() and pause() ask the host and fire play and pause; its news keeps currentTime and fires the rest", () => {
    const host = fresh();
    let v: any;
    const seen: string[] = [];
    const ev = (e: any) => seen.push(`${e.type}@${e.currentTarget.currentTime.toFixed(1)}`);
    function App() {
      const r = useRef<any>(null);
      useEffect(() => void (v = r.current), []);
      return <video ref={r} data-testid="v" src="/media/renders/film.mp4" muted loop onPlay={ev} onPause={ev} onPlaying={ev} onTimeUpdate={ev} onEnded={ev} onCanPlay={ev} />;
    }
    mount(<App />, 0, { dom: true });
    const id = host.tid("v");
    const prop = (k: string) => (flush(), host.nodes.get(id)!.props[k]);
    const news = (type: string, extra: Record<string, unknown> = {}) => flushSync(() => host.named("media", { id, type, ...extra }));
    expect(v.paused).toBe(true);
    expect(v.readyState).toBe(0);
    news("loadedmetadata", { w: 1920, h: 1080, duration: 120.6 });
    news("loadeddata", { w: 1920, h: 1080, duration: 120.6 });
    expect(v.readyState).toBe(2);
    expect(v.duration).toBe(120.6);

    const played = v.play();
    expect(played).toBeInstanceOf(Promise);
    expect(v.paused).toBe(false);
    expect(prop("__play")).toEqual([true, 0, 1, true]);
    expect(prop("__muted")).toBe(true);
    news("playing", { time: 0 });
    // each frame's time is the element's currentTime, with no event; timeupdate comes on its own
    news("time", { time: 1.5 });
    expect(v.currentTime).toBe(1.5);
    expect(v.readyState).toBe(2);
    news("timeupdate", { time: 2 });
    v.muted = false;
    expect(prop("__muted")).toBe(false);
    v.muted = true;
    v.pause();
    expect(v.paused).toBe(true);
    expect(prop("__play")).toEqual([false, 2, 2, true]);

    // a new source pauses while it loads, as a browser's does, and its readiness starts again
    v.play();
    v.setAttribute("src", "/media/renders/other.mp4");
    expect(v.paused).toBe(true);
    expect(v.readyState).toBe(0);
    expect(prop("__play")).toEqual([false, 2, 4, true]);

    // played to its end (the host sends ended for a source not looping): paused there, then ended
    v.play();
    news("ended", { time: 120.6 });
    expect(v.paused).toBe(true);
    expect(v.ended).toBe(true);
    expect(v.currentTime).toBe(120.6);
    // played again from the end starts at the beginning, and is there from the call on (a pause before the host's first
    // frame pauses it at the start, as a browser's does, not back at the end)
    v.play();
    expect(prop("__play")).toEqual([true, 0, 6, true]);
    expect(v.currentTime).toBe(0);
    expect(seen).toEqual(["canplay@0.0", "play@0.0", "playing@0.0", "timeupdate@2.0", "pause@2.0", "play@2.0", "pause@2.0", "play@2.0", "pause@120.6", "ended@120.6", "play@0.0"]);

    // load(): back to the start, paused with no pause event, and the host's decoder opened again
    v.load();
    expect(v.paused).toBe(true);
    expect(v.currentTime).toBe(0);
    expect(v.readyState).toBe(0);
    expect(prop("__load")).toBe(1);
    expect(prop("__play")).toEqual([false, 0, 7, true]);
    expect(seen.at(-1)).toBe("play@0.0");
    expect(v.canPlayType("video/mp4")).toBe("maybe");
    expect(v.canPlayType("video/x-unknown")).toBe("");
  });

  test("React's muted reaches the host's player when it changes, as React DOM sets the property", () => {
    const host = fresh();
    let setMuted: (m: boolean) => void = () => {};
    function App() {
      const [muted, set] = useState(true);
      setMuted = set;
      return <video data-testid="v" src="/media/renders/film.mp4" muted={muted} />;
    }
    mount(<App />, 0, { dom: true });
    const id = host.tid("v");
    flush();
    expect(host.nodes.get(id)!.props.__muted).toBe(true);
    flushSync(() => setMuted(false));
    flush();
    expect(host.nodes.get(id)!.props.__muted).toBe(false);
  });

  test("volume reaches the host's player as __volume, fires volumechange, and is told again with a play", () => {
    const host = fresh();
    let v: any;
    const seen: string[] = [];
    function App() {
      const r = useRef<any>(null);
      useEffect(() => void (v = r.current), []);
      return <video ref={r} data-testid="v" src="/media/renders/film.mp4" onVolumeChange={(e: any) => seen.push(`${e.type}:${e.currentTarget.volume}`)} />;
    }
    mount(<App />, 0, { dom: true });
    const id = host.tid("v");
    const prop = (k: string) => (flush(), host.nodes.get(id)!.props[k]);
    expect(v.volume).toBe(1);
    expect(prop("__volume")).toBeUndefined();
    v.volume = 0.4;
    expect(v.volume).toBe(0.4);
    expect(prop("__volume")).toBe(0.4);
    v.volume = 0.4;
    expect(() => (v.volume = 1.5)).toThrow(RangeError);
    expect(v.volume).toBe(0.4);
    v.play();
    expect(prop("__volume")).toBe(0.4);
    expect(seen).toEqual(["volumechange:0.4"]);
  });
});

// getComputedStyle on a React page: the boxes and the classes libraries read come from the page's own cascade, with
// no host call; a colour or a custom property, which that cascade can't know (a class's color, a var()), is the
// host's (stylo's style.computed), as an HTML page's every value is
describe("P18 DOM surface: computed style", () => {
  test("a React page asks the host for a colour and a custom property, never for display or a size", () => {
    const host = fresh();
    let el: any;
    mount(<span data-testid="gold" className="block text-gold" ref={(e: any) => (el = e)} />, 0, { dom: true });
    snap(host, { gold: [0, 0, 40, 20] });
    const asked: { name: string; args: any }[] = [];
    const was = g.__ui_sync;
    g.__ui_sync = (name: string, args: string) => {
      const a = JSON.parse(args);
      asked.push({ name, args: a });
      const v: Record<string, string> = { color: "rgb(240, 180, 40)", "background-color": "rgba(0, 0, 0, 0)", "--accent": "#5b8cff" };
      return JSON.stringify({ ok: { [a.names[0]]: v[a.names[0]] ?? "" } });
    };
    let got: { display: string; askedForDisplay: number; color: string; accent: string; background: string };
    try {
      const c = getComputedStyle(el);
      const display = c.display;
      got = { display, askedForDisplay: asked.length, color: c.color, accent: c.getPropertyValue("--accent"), background: c.backgroundColor };
    } finally {
      g.__ui_sync = was;
    }
    expect(got).toEqual({ display: "block", askedForDisplay: 0, color: "rgb(240, 180, 40)", accent: "#5b8cff", background: "rgba(0, 0, 0, 0)" });
    expect(asked.map((a) => [a.name, a.args.id === el.__wid, a.args.names[0]])).toEqual([
      ["style.computed", true, "color"],
      ["style.computed", true, "--accent"],
      ["style.computed", true, "background-color"],
    ]);
  });

  test("with no host to ask, a colour is the page cascade's, as before", () => {
    fresh();
    let el: any;
    mount(<span style={{ color: "red" }} ref={(e: any) => (el = e)} />, 0, { dom: true });
    expect(getComputedStyle(el).color).toBe("red");
  });
});
