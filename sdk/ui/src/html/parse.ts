// An HTML parser for compositions (docs/ts-native.feature P27): a page's markup as a plain tree, which html.ts builds
// into dom.ts elements. Not the HTML5 tree builder: tags, attributes (quoted, unquoted, bare), comments, doctype,
// character references, void elements, raw text (script, style, textarea, title), <template> content kept apart, and
// the implied end tags a hand-written page leans on (a <p> or <li> closed by the next one, a block closing an open
// <p>, an end tag closing what it encloses). Whitespace is kept as written; html.ts decides what renders.

// SVG's camelCase tag names, which HTML's parser gives back after lowercasing (the spec's "adjust SVG tag name"
// table): `<linearGradient>` is linearGradient, not lineargradient
const SVG_TAGS = new Map(
  [
    "altGlyph", "altGlyphDef", "altGlyphItem", "animateColor", "animateMotion", "animateTransform", "clipPath",
    "feBlend", "feColorMatrix", "feComponentTransfer", "feComposite", "feConvolveMatrix", "feDiffuseLighting",
    "feDisplacementMap", "feDistantLight", "feDropShadow", "feFlood", "feFuncA", "feFuncB", "feFuncG", "feFuncR",
    "feGaussianBlur", "feImage", "feMerge", "feMergeNode", "feMorphology", "feOffset", "fePointLight",
    "feSpecularLighting", "feSpotLight", "feTile", "feTurbulence", "foreignObject", "glyphRef", "linearGradient",
    "radialGradient", "textPath",
  ].map((n): [string, string] => [n.toLowerCase(), n]),
);
// and its camelCase attribute names ("adjust SVG attributes"): viewBox, gradientUnits, preserveAspectRatio...
const SVG_ATTRS = new Map(
  [
    "attributeName", "attributeType", "baseFrequency", "baseProfile", "calcMode", "clipPathUnits", "diffuseConstant",
    "edgeMode", "filterUnits", "glyphRef", "gradientTransform", "gradientUnits", "kernelMatrix", "kernelUnitLength",
    "keyPoints", "keySplines", "keyTimes", "lengthAdjust", "limitingConeAngle", "markerHeight", "markerUnits",
    "markerWidth", "maskContentUnits", "maskUnits", "numOctaves", "pathLength", "patternContentUnits",
    "patternTransform", "patternUnits", "pointsAtX", "pointsAtY", "pointsAtZ", "preserveAlpha", "preserveAspectRatio",
    "primitiveUnits", "refX", "refY", "repeatCount", "repeatDur", "requiredExtensions", "requiredFeatures",
    "specularConstant", "specularExponent", "spreadMethod", "startOffset", "stdDeviation", "stitchTiles",
    "surfaceScale", "systemLanguage", "tableValues", "targetX", "targetY", "textLength", "viewBox", "viewTarget",
    "xChannelSelector", "yChannelSelector", "zoomAndPan",
  ].map((n): [string, string] => [n.toLowerCase(), n]),
);
const svgTag = (t: string) => SVG_TAGS.get(t) ?? t;

export type HNode = HElement | HText;
export type HText = { type: "text"; text: string };
export type HElement = { type: "el"; tag: string; attrs: [string, string][]; kids: HNode[]; content?: HElement };

/** elements with no end tag */
export const VOID = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"]);
const RAW = new Set(["script", "style", "textarea", "title", "xmp"]);
// a start tag of one of these closes an open <p>
const CLOSES_P = new Set(["address", "article", "aside", "blockquote", "details", "div", "dl", "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "main", "menu", "nav", "ol", "p", "pre", "section", "table", "ul"]);

const NAMED: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", copy: "©", reg: "®", trade: "™", hellip: "…", mdash: "—", ndash: "–", lsquo: "‘", rsquo: "’", ldquo: "“", rdquo: "”", bull: "•", middot: "·", times: "×", divide: "÷", deg: "°", rarr: "→", larr: "←", uarr: "↑", darr: "↓", euro: "€", pound: "£", yen: "¥", cent: "¢", sect: "§", para: "¶", laquo: "«", raquo: "»", thinsp: " ", ensp: " ", emsp: " ", zwj: "‍", zwnj: "‌", shy: "­", Tab: "\t", NewLine: "\n", ZeroWidthSpace: "\u200b", NoBreak: "\u2060", hairsp: "\u200a", numsp: "\u2007", puncsp: "\u2008", lrm: "\u200e", rlm: "\u200f", hyphen: "\u2010", dash: "\u2010", minus: "\u2212", iexcl: "¡", iquest: "¿", micro: "µ", plusmn: "±", frac12: "½", frac14: "¼", frac34: "¾", sup2: "²", sup3: "³", eacute: "é", Eacute: "É", aacute: "á", agrave: "à", egrave: "è", uuml: "ü", ouml: "ö", auml: "ä", szlig: "ß", ccedil: "ç", ntilde: "ñ", alpha: "α", beta: "β", pi: "π", Omega: "Ω", check: "✓", star: "☆", hearts: "♥" };

/** Character references (`&amp;`, `&#39;`, `&#x27;`) decoded. */
export function decode(s: string): string {
  if (!s.includes("&")) return s;
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z][a-z0-9]*);?/gi, (m, r: string) => {
    if (r[0] === "#") {
      const n = r[1] === "x" || r[1] === "X" ? parseInt(r.slice(2), 16) : parseInt(r.slice(1), 10);
      return Number.isFinite(n) && n > 0 && n <= 0x10ffff ? String.fromCodePoint(n) : m;
    }
    const v = NAMED[r] ?? NAMED[r.toLowerCase()];
    return v ?? m;
  });
}

/** The document: a root element (`#document`) whose kids are what the markup has at its top. */
export function parse(src: string): HElement {
  const doc: HElement = { type: "el", tag: "#document", attrs: [], kids: [] };
  const stack: HElement[] = [doc];
  const top = () => stack[stack.length - 1];
  const close = (tag: string) => {
    for (let i = stack.length - 1; i > 0; i--) {
      if (stack[i].tag === tag) {
        stack.length = i;
        return;
      }
    }
  };
  const inScope = (tag: string) => stack.some((e) => e.tag === tag);
  const add = (n: HNode) => {
    const t = top();
    (t.content ?? t).kids.push(n);
  };
  let i = 0;
  const n = src.length;
  while (i < n) {
    const lt = src.indexOf("<", i);
    if (lt < 0) {
      add({ type: "text", text: decode(src.slice(i)) });
      break;
    }
    if (lt > i) add({ type: "text", text: decode(src.slice(i, lt)) });
    i = lt;
    if (src.startsWith("<!--", i)) {
      const e = src.indexOf("-->", i + 4);
      i = e < 0 ? n : e + 3;
      continue;
    }
    if (src[i + 1] === "!" || src[i + 1] === "?") {
      const e = src.indexOf(">", i);
      i = e < 0 ? n : e + 1;
      continue;
    }
    if (src[i + 1] === "/") {
      const e = src.indexOf(">", i);
      const tag = svgTag(src.slice(i + 2, e < 0 ? n : e).trim().toLowerCase());
      i = e < 0 ? n : e + 1;
      if (tag === "template") {
        // the template's content holder was pushed with it
        close("template");
        continue;
      }
      if (inScope(tag)) close(tag);
      continue;
    }
    // a start tag
    const m = /^<([a-zA-Z][a-zA-Z0-9:_-]*)/.exec(src.slice(i, i + 64));
    if (!m) {
      add({ type: "text", text: "<" });
      i++;
      continue;
    }
    const tag = svgTag(m[1].toLowerCase());
    i += m[0].length;
    const attrs: [string, string][] = [];
    let selfClose = false;
    while (i < n) {
      while (i < n && /\s/.test(src[i])) i++;
      if (src[i] === ">") {
        i++;
        break;
      }
      if (src[i] === "/" && src[i + 1] === ">") {
        selfClose = true;
        i += 2;
        break;
      }
      if (src[i] === "/") {
        i++;
        continue;
      }
      const a = /^[^\s/>=]+/.exec(src.slice(i, i + 256));
      if (!a) {
        i++;
        continue;
      }
      const name = SVG_ATTRS.get(a[0].toLowerCase()) ?? a[0].toLowerCase();
      i += a[0].length;
      while (i < n && /\s/.test(src[i])) i++;
      let value = "";
      if (src[i] === "=") {
        i++;
        while (i < n && /\s/.test(src[i])) i++;
        const q = src[i];
        if (q === '"' || q === "'") {
          const e = src.indexOf(q, i + 1);
          value = src.slice(i + 1, e < 0 ? n : e);
          i = e < 0 ? n : e + 1;
        } else {
          const v = /^[^\s>]*/.exec(src.slice(i))![0];
          value = v;
          i += v.length;
        }
      }
      if (!attrs.some(([k]) => k === name)) attrs.push([name, decode(value)]);
    }
    // implied end tags
    if (CLOSES_P.has(tag) && inScope("p")) close("p");
    if (tag === "li" && top().tag === "li") close("li");
    if ((tag === "dt" || tag === "dd") && (top().tag === "dt" || top().tag === "dd")) stack.length--;
    if (tag === "option" && top().tag === "option") stack.length--;
    const el: HElement = { type: "el", tag, attrs, kids: [] };
    add(el);
    if (RAW.has(tag)) {
      const re = new RegExp(`</${tag}\\s*>`, "i");
      const rest = src.slice(i);
      const e = rest.search(re);
      const text = e < 0 ? rest : rest.slice(0, e);
      if (text) el.kids.push({ type: "text", text: tag === "textarea" || tag === "title" ? decode(text) : text });
      i += e < 0 ? rest.length : e + rest.slice(e).indexOf(">") + 1;
      continue;
    }
    if (tag === "template") {
      el.content = { type: "el", tag: "#fragment", attrs: [], kids: [] };
      stack.push(el);
      continue;
    }
    if (VOID.has(tag) || selfClose) continue;
    stack.push(el);
  }
  return doc;
}

/** The first element named `tag` under `n` (n itself included), in document order; template content skipped. */
export function find(n: HElement, tag: string): HElement | undefined {
  if (n.tag === tag) return n;
  for (const k of n.kids) {
    if (k.type !== "el") continue;
    const f = find(k, tag);
    if (f) return f;
  }
  return undefined;
}
/** Every element under `n` (itself included) in document order, template content skipped. */
export function elements(n: HElement, out: HElement[] = []): HElement[] {
  out.push(n);
  for (const k of n.kids) if (k.type === "el") elements(k, out);
  return out;
}
export const attr = (e: HElement, k: string): string | undefined => e.attrs.find(([a]) => a === k)?.[1];
export function setAttr(e: HElement, k: string, v: string | undefined) {
  const i = e.attrs.findIndex(([a]) => a === k);
  if (v === undefined) {
    if (i >= 0) e.attrs.splice(i, 1);
  } else if (i >= 0) e.attrs[i][1] = v;
  else e.attrs.push([k, v]);
}
export function textOf(n: HNode): string {
  return n.type === "text" ? n.text : n.kids.map(textOf).join("");
}
