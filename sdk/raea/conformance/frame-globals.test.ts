// requestAnimationFrame and Image where a composition's runtime has none (#56): callbacks run once per frame, given the
// frame's time, and a preload's load fires.
import { expect, test } from "bun:test";

const g = globalThis as any;
const had = { raf: g.requestAnimationFrame, caf: g.cancelAnimationFrame, Image: g.Image };
delete g.requestAnimationFrame;
delete g.cancelAnimationFrame;
delete g.Image;
const { frameCallbacks } = await import("../src/frame-globals");
const ours = { raf: g.requestAnimationFrame, caf: g.cancelAnimationFrame, Image: g.Image };
Object.assign(g, { requestAnimationFrame: had.raf, cancelAnimationFrame: had.caf, Image: had.Image });

test("a frame callback runs once, at the next frame, with that frame's time", () => {
  const got: number[] = [];
  ours.raf((t: number) => got.push(t));
  const gone = ours.raf(() => got.push(-1));
  ours.caf(gone);
  frameCallbacks(3, 30);
  frameCallbacks(3, 30);
  frameCallbacks(4, 30);
  expect(got).toEqual([100]);
});

test("a callback asked for while one runs waits for the frame after", () => {
  const got: number[] = [];
  ours.raf(() => ours.raf((t: number) => got.push(t)));
  frameCallbacks(10, 25);
  expect(got).toEqual([]);
  frameCallbacks(11, 25);
  expect(got).toEqual([440]);
});

test("an Image's load fires once its src is set", async () => {
  const img = new ours.Image();
  const seen: string[] = [];
  img.onload = () => seen.push("onload");
  img.addEventListener("load", () => seen.push("listener"));
  img.src = "/media/x.png";
  expect(img.complete).toBe(false);
  await Promise.resolve();
  expect([seen, img.complete, img.src]).toEqual([["onload", "listener"], true, "/media/x.png"]);
});

// #253: a loaded Image has the picture's size (a canvas sized from it was 0 high, and its CanvasTexture refused)
const host = globalThis as { __raea_call?: (n: string, j: string) => string | undefined };
const loaded = (img: any) => new Promise<string>((ok) => {
  img.onload = () => ok("load");
  img.onerror = () => ok("error");
});

test("a loaded Image's width, height and natural size are the picture's, from the host (asset.load)", async () => {
  const calls: string[] = [];
  host.__raea_call = (name, json) => {
    calls.push(`${name} ${json}`);
    return name === "asset.load" ? JSON.stringify({ path: "/edit/public/site.png", bytes: 999, width: 400, height: 300 }) : undefined;
  };
  try {
    const img = new ours.Image();
    const done = loaded(img);
    img.src = "/media/site.png";
    expect([img.naturalWidth, img.width]).toEqual([0, 0]);
    expect(await done).toBe("load");
    expect([img.width, img.height, img.naturalWidth, img.naturalHeight]).toEqual([400, 300, 400, 300]);
    expect(calls).toEqual([`asset.load {"path":"/media/site.png"}`]);
    // width and height given (the constructor's, or set) stay as given; the natural size is still the picture's
    const sized = new ours.Image(50, 20);
    const both = loaded(sized);
    sized.src = "/media/site.png";
    await both;
    expect([sized.width, sized.height, sized.naturalWidth, sized.naturalHeight]).toEqual([50, 20, 400, 300]);
    sized.height = 10;
    expect([sized.width, sized.height]).toEqual([50, 10]);
  } finally {
    delete host.__raea_call;
  }
});

test("a data: URL Image's size comes from its header; a file the host can't find fires error", async () => {
  // a 3 x 2 PNG
  const png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAYAAACddGYaAAAAEklEQVR4nGP4z8DwH4YZYBwAaG8H+VKwN2UAAAAASUVORK5CYII=";
  const img = new ours.Image();
  const done = loaded(img);
  img.src = png;
  expect(await done).toBe("load");
  expect([img.width, img.height, img.naturalWidth, img.naturalHeight]).toEqual([3, 2, 3, 2]);
  host.__raea_call = () => {
    throw new Error("asset.load: asset /media/gone.png: No such file or directory");
  };
  try {
    const gone = new ours.Image();
    const failed = loaded(gone);
    gone.src = "/media/gone.png";
    expect(await failed).toBe("error");
    expect([gone.complete, gone.naturalWidth, gone.width]).toEqual([true, 0, 0]);
  } finally {
    delete host.__raea_call;
  }
});
