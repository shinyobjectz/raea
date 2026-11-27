---
title: "Web components that measure"
summary: "What a composition's DOM answers, as Chrome does: refs' geometry, style and tree, document and window, canvas and svg text measures, Range, ResizeObserver, and the frame held until a measure lands. And what it lacks."
order: 4
sources: ["docs/layout-reads.feature", "sdk/raea/src/dom-reads.ts", "sdk/raea/src/dom-tree.ts", "sdk/raea/src/dom-style.ts", "sdk/raea/src/dom-document.ts", "sdk/raea/src/dom-range.ts", "sdk/raea/src/dom-svgtext.ts", "sdk/raea/src/dom-observers.ts", "sdk/raea/src/dom-load.ts", "sdk/raea/src/canvas2d.ts", "sdk/raea/src/frame-globals.ts", "sdk/raea/src/hold.ts"]
---

Many components written for a web page measure before they draw: a title sized to fit its box, captions that wrap at
the width a line really takes, a caret placed after the last typed letter. In a composition those reads answer as they
do in Chrome, from the same layout and the same font files that draw the frame. Nothing needs importing: the entry
installs them on every ref and on `document` and `window`.

## Measure, then let the frame be taken

A measure runs in an effect, after the first commit. Hold the frame until it's done, so the frame taken is the measured
one:

```tsx
import { continueRender, delayRender } from "raea";
import { loadFont } from "raea/text";

loadFont("Inter");

export const Fit = ({ text }: { text: string }) => {
  const ref = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [handle] = useState(() => delayRender("fit the title"));
  useEffect(() => {
    document.fonts.ready.then(() => {
      setScale(1080 / ref.current!.getBoundingClientRect().width);
      continueRender(handle);
    });
  }, []);
  return <div ref={ref} style={{ display: "inline-block", fontFamily: "Inter", transform: `scale(${scale})` }}>{text}</div>;
};
```

A hold never released fails the render after 30 s, naming its label ([raea/hold](../reference/sdk/hold.md)). Load a
web font with `loadFont` so the measure uses its own faces; a family you never asked for is measured in raea's bundled
faces, as a browser falls back.

## What a ref answers

| | |
|---|---|
| Geometry | `getBoundingClientRect()` (transforms included), `getClientRects()`, `offsetWidth`, `offsetHeight`, `offsetLeft`, `offsetTop`, `offsetParent`, `client*`, `scroll*`. Values are Chrome's, in the composition's CSS px. |
| Style | `el.style` reads what React committed. Writing it (a property, `setProperty`, `cssText`) restyles the element at once; React's next commit to the same property wins. `getComputedStyle(el)` answers from the page's computed style. |
| The tree | `children`, `childNodes`, the sibling and parent links, `tagName`, `id`, `className`, `classList`, `dataset`, `getAttribute`, `textContent`, `contains`, `matches`, `closest`, `querySelector` and `querySelectorAll`. |
| svg text | `getComputedTextLength`, `getNumberOfChars`, `getStartPositionOfChar`, `getEndPositionOfChar` and `getSubStringLength`, in the element's computed font, letter- and word-spacing and font-variation-settings included, hidden text too. |
| canvas | `getContext("2d")` with `measureText` (width, the ink's and the font's boxes, shaped from the faces the canvas draws with), `letterSpacing` and `wordSpacing`, `getImageData` and `putImageData`. |
| images | An `<img>`'s `onLoad` fires once it's made or its `src` changes; the frame still waits for the picture itself. |

Selectors cover tag, `*`, `#id`, `.class`, `[attr]` and `[attr=v]` (and `~=`, `^=`, `$=`, `*=`), the descendant and
child combinators, comma lists, `:first-child`, `:last-child`, `:only-child`, `:nth-child()` and `:not()`. Anything
else throws a SyntaxError, as Chrome's does.

## document and window

- `document.body` is the composition's root. `getElementById`, `querySelector` and `querySelectorAll` search it.
- `document.createElement` makes an element of the page: appended into the composition it's laid out, measured and
  drawn, and removed it's gone. That's how a component measures a hidden copy. A canvas made there is an
  OffscreenCanvas with the same 2D context.
- `document.createRange()` over a text node gives the rect of its characters, placed where the line starts. One line
  only: a range across wrapped lines gets one rect, the width of its characters.
- `document.fonts` has `ready`, `load` and `check`; a `FontFace` given by URL is fetched and its text set in it. One
  that doesn't load rejects, and the text falls back.
- `window` has `innerWidth`, `innerHeight` and `devicePixelRatio` for the composition's size, and `getComputedStyle`.
- `requestAnimationFrame` calls back once per frame drawn, given that frame's time, so a frame drawn again gets the
  same calls. `new Image()` fires `load` when its `src` is set (a `staticFile` path, a data: URL or an http URL), and
  by then `naturalWidth` and `naturalHeight` are the picture's size; `width` and `height` are the ones you gave, else
  the same. A file that isn't there fires `error` instead.
- `ResizeObserver` calls back once when it starts observing, then after any layout that changed an observed element's
  size, with `contentRect`, `borderBoxSize` and `contentBoxSize`.

## What it lacks

- `MutationObserver` and `IntersectionObserver` exist, so a component that makes one doesn't throw, but they never
  call back.
- An `<img>`'s `onError` never fires: an image that doesn't load fails the render, naming its src.
- An element has no `innerHTML` or `outerHTML`: read `textContent`, or walk `children` and `childNodes`.
- A Range across wrapped lines is one rect, not one per line, and an inline element's `getClientRects()` is one rect
  too.
- In a pass that draws no page (the sound pass, a 3D pre-pass) every read answers 0, and holds there are released.
- Inside a `RuneLayer` in an app, `getBoundingClientRect` is zero.

Measured against Chrome on snapcn's measuring components, `bench/layout-reads/` in the repository is the harness that
holds these to it ([#54](https://github.com/shinyobjectz/raea/issues/54)).
