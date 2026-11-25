// The DOM-like surface as raea cloud's dashboard uses it in raea.app (markup set as HTML, sheets as <dialog>s, a second
// React root over the page, properties set on fields it makes), on the stand-in host (dom.test.tsx's).
import { beforeAll, describe, expect, test } from "bun:test";
import { useEffect, useRef } from "react";
import { install } from "../src/dom";
import { mount } from "../src/react";
import { flush } from "../src/wire";
import { fresh, snap } from "./host";

beforeAll(() => install());
const g = globalThis as any;

const press = (x: number, y: number, detail = 1) => ({ x: 0, y: 0, clientX: x, clientY: y, button: 0, detail });

describe("a dashboard's page", () => {
  test("a link's press follows it: its fragment becomes location's hash, unless a listener prevents it", () => {
    const host = fresh();
    function App() {
      const r = useRef<any>(null);
      // markup set as HTML, as raea cloud's dashboard builds its rail in raea.app
      useEffect(() => void (r.current.innerHTML = '<a data-testid="go" href="#/projects">Projects</a><a data-testid="stay" href="#/agents">Agents</a>'), []);
      return <nav data-testid="root" ref={r} onClick={() => {}} />;
    }
    mount(<App />, 0, { dom: true });
    flush();
    snap(host, { root: [0, 0, 200, 100], go: [0, 0, 200, 40], stay: [0, 50, 200, 40] });
    const was = g.location.hash;
    host.send("root", "pointerdown", { ...press(10, 10), target: host.tid("go") });
    host.send("root", "pointerup", { ...press(10, 10), target: host.tid("go") });
    expect(g.location.hash).toBe("#/projects");
    g.document.querySelector('[data-testid="stay"]').addEventListener("click", (e: any) => e.preventDefault());
    host.send("root", "pointerdown", { ...press(10, 60), target: host.tid("stay") });
    host.send("root", "pointerup", { ...press(10, 60), target: host.tid("stay") });
    expect(g.location.hash).toBe("#/projects");
    g.location.hash = was;
  });

  test("a dialog opened with showModal (raea cloud's sheets): open, its first control focused, Escape's cancel closes it, close fires after", async () => {
    fresh();
    mount(<main data-testid="page">dashboard</main>, 0, { dom: true });
    flush();
    const d = g.document.createElement("dialog");
    d.innerHTML = '<p>New</p><button type="button" data-testid="first">Project</button>';
    g.document.body.append(d);
    const seen: string[] = [];
    d.addEventListener("cancel", () => seen.push("cancel"));
    d.addEventListener("close", () => seen.push("close"));
    d.showModal();
    flush();
    expect(d.open).toBe(true);
    expect(d.hasAttribute("open")).toBe(true);
    expect(g.document.activeElement).toBe(d.querySelector("button"));
    // Escape at what's focused: cancel, then closed; close comes after, as a task
    g.__ui_deliver(JSON.stringify({ t: "named", name: "keydown", value: { key: "Escape" } }));
    expect(d.open).toBe(false);
    expect(seen).toEqual(["cancel"]);
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual(["cancel", "close"]);
    // a prevented cancel keeps it open; close() keeps its returnValue
    d.showModal();
    d.addEventListener("cancel", (e: any) => e.preventDefault());
    g.__ui_deliver(JSON.stringify({ t: "named", name: "keydown", value: { key: "Escape" } }));
    expect(d.open).toBe(true);
    d.close("done");
    expect(d.open).toBe(false);
    expect(d.returnValue).toBe("done");
    d.remove();
  });

  test("a second React root on a layer over the page (raea.app's welcome) takes its button's click", () => {
    const host = fresh();
    const log: string[] = [];
    mount(<main data-testid="page" onClick={() => log.push("page")}>dashboard</main>, 0, { dom: true });
    flush();
    const layer = g.document.createElement("div");
    layer.setAttribute("data-testid", "layer");
    g.document.body.appendChild(layer);
    flush();
    mount(
      <div data-testid="over" className="fixed inset-0">
        <button data-testid="start" onClick={() => log.push("start")}>Start</button>
      </div>,
      layer.__wid,
      { dom: true },
    );
    flush();
    snap(host, { page: [0, 0, 800, 600], layer: [0, 0, 0, 0], over: [0, 0, 800, 600], start: [600, 500, 60, 30] });
    const target = host.tid("start");
    // to the nearest element up from the button that listens, as the host sends it
    const hearer = ["start", "over", "layer", "page"].find((t) => (host as any).get(host.tid(t)).on.has("pointerdown"));
    host.send(hearer!, "pointerdown", { ...press(620, 510), target });
    host.send(hearer!, "pointerup", { ...press(620, 510), target });
    expect(log).toEqual(["start"]);
  });
});

describe("outerHTML", () => {
  test("set, the element is replaced by its markup in its place; out of a tree, nothing happens", () => {
    fresh();
    let box: any;
    mount(<div ref={(e: any) => (box = e)} />, 0, { dom: true });
    box.innerHTML = "<header class='a'><b>one</b></header><p>after</p>";
    box.firstChild.outerHTML = "<header class='b'><i>two</i></header>";
    expect(box.innerHTML).toBe('<header class="b"><i>two</i></header><p>after</p>');
    box.lastChild.outerHTML = "";
    expect(box.innerHTML).toBe('<header class="b"><i>two</i></header>');
    const off: any = document.createElement("u");
    off.outerHTML = "<s></s>";
    expect(off.outerHTML).toBe("<u></u>");
  });
});

describe("writable properties", () => {
  test("a property set on a field made by the page reaches its attribute, as Chrome's do", () => {
    fresh();
    let box: any;
    mount(<div ref={(e: any) => (box = e)} />, 0, { dom: true });
    const input: any = document.createElement("input");
    box.append(input);
    input.placeholder = "Search";
    input.required = true;
    input.readOnly = true;
    input.readOnly = false;
    expect(input.outerHTML).toBe('<input placeholder="Search" required="">');
    const a: any = document.createElement("a");
    a.href = "#/projects";
    a.draggable = false;
    expect([a.getAttribute("href"), a.getAttribute("draggable")]).toEqual(["#/projects", "false"]);
    const label: any = document.createElement("label");
    label.htmlFor = "q";
    expect(label.getAttribute("for")).toBe("q");
    box.innerHTML = "<select><option value='a'>A</option><option value='b'>B</option></select>";
    box.firstChild.selectedIndex = 1;
    expect([box.firstChild.value, box.firstChild.selectedIndex]).toEqual(["b", 1]);
  });
});
