// bun void/parts/brand.ts SPACE [INPUT.json]   (input: {brand, styleguide, logos}; stdin when no file)
// The brand tool's writer: turns the host app's brand APIs (/v1/brands/retrieve for name, slogan, palette and logos;
// /v1/brands/styleguide, context.dev's tokens from the brand's website) into the space's brand kit:
//   brand/styleguide.json  every token, as the host app writes it (pretty, sorted keys)
//   brand/brand.css        the host app's brand.css, made the same way (BrandKit.make in host/shared/BrandKit.swift), so
//                          a kit moves between the host app and raea
//   brand/brand.json       what the parts kit reads, derived from the two above: colours by role (picked by contrast,
//                          only from the brand's own colours), heading and body fonts, the type scale, the CTA button
//   BRAND.md               the same for the agent to read
// Nothing is invented: a role no brand colour fills legibly is left null, for the agent to decide.
import { contrast } from "./kit";

const space = process.argv[2];
if (!space) throw new Error("usage: bun brand.ts SPACE [INPUT.json]");
const input = JSON.parse(process.argv[3] ? await Bun.file(process.argv[3]).text() : await Bun.stdin.text());
const b: any = input.brand ?? {};
const sg: any = input.styleguide ?? null;
const logos: any[] = input.logos ?? [];
const domain: string = b.domain ?? input.domain ?? "";
const name: string = b.name ?? domain;

// ---- brand.css, as the host app makes it -------------------------------------------------------------------------------
const lookAlikes: Record<string, string> = {
  "helvetica": "Inter", "neue haas": "Inter", "graphik": "Inter", "sf pro": "Inter", "aktiv": "Inter",
  "apercu": "DM Sans", "circular": "DM Sans", "gt walsheim": "DM Sans", "gt america": "Inter", "founders grotesk": "Inter",
  "futura": "Jost", "avenir": "Nunito Sans", "gotham": "Montserrat", "proxima nova": "Montserrat", "brandon": "Josefin Sans",
  "gilroy": "Plus Jakarta Sans", "sofia pro": "Plus Jakarta Sans", "euclid": "Plus Jakarta Sans", "maison neue": "Inter",
  "basis grotesque": "Inter", "suisse": "Inter", "untitled sans": "Inter", "styrene": "Space Grotesk", "neue montreal": "Inter",
  "canela": "Playfair Display", "didot": "Playfair Display", "bodoni": "Bodoni Moda", "garamond": "EB Garamond",
  "caslon": "Libre Caslon Text", "tiempos": "Source Serif 4", "freight": "Source Serif 4", "gt super": "Playfair Display",
  "recoleta": "Fraunces", "cooper": "Fraunces", "chronicle": "Source Serif 4",
};
// Swift iterates a Dictionary in no fixed order for lookAlikes.first(where:); keys rarely overlap, so first match in
// declaration order is the same answer in practice.
function googleFamily(family: string, link: any, fallbacks: string[]): string {
  if (link?.type === "google") return family;
  const key = family.toLowerCase();
  const match = Object.entries(lookAlikes).find(([k]) => key.includes(k));
  if (match) return match[1];
  const generic = new Set(fallbacks.map((f) => f.toLowerCase()));
  if (generic.has("monospace")) return "JetBrains Mono";
  if (generic.has("serif") && !generic.has("sans-serif")) return "Lora";
  return "Inter";
}
function slug(text: string): string {
  const folded = text.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");
  let out = "";
  for (const ch of folded) {
    if (/^[a-z0-9]$/.test(ch)) out += ch;
    else if (out && !out.endsWith("-")) out += "-";
  }
  out = out.replace(/-+$/, "");
  return out || "untitled";
}
const sortedKeys = (o: any): any => Array.isArray(o) ? o.map(sortedKeys)
  : o && typeof o === "object" ? Object.fromEntries(Object.keys(o).sort().map((k) => [k, sortedKeys(o[k])])) : o;

const palette: string[] = (b.colors ?? []).map((c: any) => (typeof c === "string" ? c : c?.hex)).filter(Boolean);
const fontLinks: Record<string, any> = sg?.fontLinks ?? {};
const typography = sg?.typography ?? {};
const styles: [string, any][] = [];
if (typography.headings) for (const k of Object.keys(typography.headings).sort()) styles.push([k, typography.headings[k]]);
if (typography.p) styles.push(["p", typography.p]);
const families: Record<string, string> = {};
const weights: Record<string, Set<number>> = {};
for (const [, style] of styles) {
  const family = style.fontFamily;
  if (typeof family !== "string" || !family) continue;
  const kit = families[family] ?? googleFamily(family, fontLinks[family], style.fontFallbacks ?? []);
  families[family] = kit;
  (weights[kit] ??= new Set()).add(Math.round(Number(style.fontWeight ?? 400)));
}
const stack = (style: any) => {
  const site = style.fontFamily ?? "";
  const kit = families[site] ?? site;
  const generic = (style.fontFallbacks ?? []).filter((f: string) => ["serif", "sans-serif", "monospace", "cursive", "system-ui"].includes(f));
  return [`"${kit}"`, ...(generic.length ? generic : ["sans-serif"])].join(", ");
};

let css = "";
if (sg) {
  const date = new Date().toISOString().slice(0, 10);
  css = `/* ${name} brand kit for the host app, from ${domain} (context.dev styleguide, ${date}).
   Link it from an edit:  <link rel="stylesheet" href="../assets/brand/brand.css">
   Sizes are the website's; on a 1080x1920 video, scale them up (about 2.5x for captions and titles).
   Edit freely: nothing overwrites this file unless the brand kit is refreshed. */
`;
  const wk = Object.keys(weights).sort();
  if (wk.length) {
    const q = wk.map((f) => `family=${f.replace(/ /g, "+")}:wght@${[...weights[f]].sort((x, y) => x - y).join(";")}`).join("&");
    css += `@import url("https://fonts.googleapis.com/css2?${q}&display=swap");\n\n`;
  }
  const vars: string[] = [];
  const sgColors: Record<string, string> = sg.colors ?? {};
  for (const k of Object.keys(sgColors).sort()) vars.push(`--brand-${slug(k)}: ${sgColors[k]};`);
  palette.forEach((hex, i) => vars.push(`--brand-color-${i + 1}: ${hex};`));
  const h1 = styles.find(([n]) => n === "h1")?.[1], p = styles.find(([n]) => n === "p")?.[1];
  if (h1) vars.push(`--brand-font-heading: ${stack(h1)};`);
  if (p) vars.push(`--brand-font: ${stack(p)};`);
  for (const [tag, style] of styles)
    for (const [key, prop] of [["fontSize", "size"], ["fontWeight", "weight"], ["lineHeight", "line"], ["letterSpacing", "tracking"]])
      if (style[key] != null) vars.push(`--brand-${tag}-${prop}: ${style[key]};`);
  for (const k of ["xs", "sm", "md", "lg", "xl"]) if (sg.elementSpacing?.[k]) vars.push(`--brand-space-${k}: ${sg.elementSpacing[k]};`);
  for (const k of ["sm", "md", "lg", "xl", "inner"]) if (sg.shadows?.[k]) vars.push(`--brand-shadow-${k}: ${sg.shadows[k]};`);
  const components = sg.components ?? {};
  if (components.card?.borderRadius != null) vars.push(`--brand-radius: ${components.card.borderRadius};`);
  css += ":root {\n" + vars.map((v) => "  " + v).join("\n") + "\n}\n";
  for (const [tag] of styles) {
    const font = tag === "p" ? "var(--brand-font)" : "var(--brand-font-heading)";
    const cls = tag === "p" ? "brand-text" : `brand-${tag}`;
    css += `\n.${cls} { font-family: ${font}; font-size: var(--brand-${tag}-size); font-weight: var(--brand-${tag}-weight); `
      + `line-height: var(--brand-${tag}-line); letter-spacing: var(--brand-${tag}-tracking); }`;
  }
  css += "\n";
  const block = (cls: string, declarations: string) => {
    let d = declarations;
    for (const [site, kit] of Object.entries(families)) if (site !== kit) d = d.split(`"${site}"`).join(`"${kit}"`).split(site).join(`"${kit}"`);
    css += `\n.${cls} { ${d.trim()} }`;
  };
  if (components.button) for (const v of Object.keys(components.button).sort()) {
    const decl = components.button[v]?.css;
    if (typeof decl === "string") block(v === "primary" ? "brand-button, .brand-button-primary" : `brand-button-${slug(v)}`, decl);
  }
  if (typeof components.card?.css === "string") block("brand-card", components.card.css);
  css += "\n";
}

// ---- brand.json: the kit's view, derived from the styleguide and the palette ---------------------------------------
const px = (v: any) => (v == null ? undefined : Number.parseFloat(String(v)));
const hex = (v: any) => (typeof v === "string" && /^#[0-9a-f]{3,8}$/i.test(v.trim()) ? v.trim().toLowerCase() : undefined);
const sgc = sg?.colors ?? {};
// every colour the brand has (site's working colours, then the palette most prominent first), each once
const pool = [...new Set([sgc.accent, sgc.background, sgc.text, ...palette].map(hex).filter(Boolean) as string[])];
const best = (against: string, from: string[], min: number) => {
  const c = from.filter((x) => x !== against).map((x) => [x, contrast(x, against)] as const).sort((p, q) => q[1] - p[1])[0];
  return c && c[1] >= min ? c[0] : null;
};
const LARGE = 3; // WCAG AA for large text: every kit part sets text >= 24px bold
const accent = hex(sgc.accent) ?? hex(palette[0]) ?? null;
const surface = hex(sgc.background) ?? null;
const textOnSurface = surface ? (hex(sgc.text) && contrast(hex(sgc.text)!, surface) >= LARGE ? hex(sgc.text)! : best(surface, pool, LARGE)) : null;
const textOnAccent = accent ? best(accent, pool, LARGE) : null;
const ratio = (a: string | null, c: string | null) => (a && c ? Math.round(contrast(a, c) * 10) / 10 : null);
const level = (style: any) => style && {
  family: families[style.fontFamily] ?? style.fontFamily ?? null, site_family: style.fontFamily ?? null,
  size: px(style.fontSize), weight: style.fontWeight != null ? Number(style.fontWeight) : undefined,
  line: px(style.lineHeight), tracking: px(style.letterSpacing),
};
const scale = Object.fromEntries(styles.map(([n, s]) => [n, level(s)]));
const btn = sg?.components?.button?.primary;
const heading = styles.find(([n]) => n === "h1")?.[1], body = styles.find(([n]) => n === "p")?.[1];
const metricsFaces = Object.keys(await Bun.file(new URL("./metrics.json", import.meta.url)).json());
const fontEntry = (style: any) => style ? {
  kit: families[style.fontFamily] ?? style.fontFamily, site: style.fontFamily,
  measured: metricsFaces.some((f) => f.toLowerCase() === String(families[style.fontFamily] ?? style.fontFamily).toLowerCase()),
} : null;

const kit = {
  name, domain, slogan: b.slogan ?? null, description: b.description ?? null, industry: b.industry ?? null,
  voice: [b.voice, b.tone, b.tagline].flat().filter((x) => typeof x === "string" && x) as string[],
  source: { styleguide: sg ? "brand/styleguide.json" : null, css: sg ? "brand/brand.css" : null, mode: sg?.mode ?? null },
  // roles map to brand.css: accent = --brand-accent, surface = --brand-background, text_on_surface = --brand-text
  palette: { primary: hex(palette[0]) ?? accent, accent, surface, text_on_surface: textOnSurface, text_on_accent: textOnAccent },
  contrast: {
    text_on_surface: ratio(textOnSurface, surface), text_on_accent: ratio(textOnAccent, accent), accent_on_surface: ratio(accent, surface),
  },
  colors: [
    ...Object.keys(sgc).sort().filter((k) => hex(sgc[k])).map((k) => ({ name: k, hex: hex(sgc[k])!, css: `--brand-${slug(k)}` })),
    ...palette.map((h, i) => ({ name: `color ${i + 1}`, hex: hex(h) ?? h, css: `--brand-color-${i + 1}` })),
  ],
  fonts: { heading: fontEntry(heading), body: fontEntry(body) },
  scale,
  button: btn ? {
    bg: hex(btn.backgroundColor) ?? null, color: hex(btn.color) ?? null, radius: px(btn.borderRadius),
    font: btn.fontFamily ? families[btn.fontFamily] ?? googleFamily(btn.fontFamily, fontLinks[btn.fontFamily], btn.fontFallbacks ?? []) : null,
    size: px(btn.fontSize), weight: btn.fontWeight != null ? Number(btn.fontWeight) : undefined,
  } : null,
  radius: px(sg?.components?.card?.borderRadius) ?? null,
  shadow: sg?.shadows?.lg && sg.shadows.lg !== "none" ? sg.shadows.lg : null,
  logos: logos.map((l) => ({ file: l.file, type: l.type ?? "logo", mode: l.mode ?? null,
    on: l.mode === "dark" ? "light" : l.mode === "light" ? "dark" : "any", width: l.width ?? null, height: l.height ?? null })),
};

// ---- write --------------------------------------------------------------------------------------------------------
const dir = `${space}/brand`;
await Bun.$`mkdir -p ${dir}`.quiet();
if (sg) {
  await Bun.write(`${dir}/styleguide.json`, JSON.stringify(sortedKeys(sg), null, 2) + "\n");
  await Bun.write(`${dir}/brand.css`, css);
}
await Bun.write(`${dir}/brand.json`, JSON.stringify(kit, null, 2) + "\n");

const role = (k: keyof typeof kit.palette, css: string) => `- ${k}: ${kit.palette[k] ?? "none (decide it yourself)"}${css}`;
const md = [
  `# ${name}${domain ? ` (${domain})` : ""}`, "",
  kit.slogan ? `"${kit.slogan}"\n` : "",
  kit.description ? `${kit.description}\n` : "",
  "Files: brand/brand.json (what the parts kit reads), brand/styleguide.json (every token from the site), brand/brand.css (the host app's stylesheet: --brand-* variables), logos in media/brand/.", "",
  "## Colours by role", "",
  role("primary", ""), role("accent", " (--brand-accent)"), role("surface", " (--brand-background)"),
  role("text_on_surface", ` (${kit.contrast.text_on_surface ?? "-"}:1 on the surface)`),
  role("text_on_accent", ` (${kit.contrast.text_on_accent ?? "-"}:1 on the accent)`),
  kit.contrast.accent_on_surface != null && kit.contrast.accent_on_surface < LARGE ? `- the accent on the surface is ${kit.contrast.accent_on_surface}:1: don't set accent text on a surface card` : "",
  "", `All colours: ${kit.colors.map((c) => `${c.name} ${c.hex}`).join(", ") || "none"}`, "",
  "## Type", "",
  ...(["heading", "body"] as const).map((k) => {
    const f = kit.fonts[k];
    if (!f) return `- ${k}: none (choose a face)`;
    return `- ${k}: ${f.kit}${f.site !== f.kit ? ` (the site uses ${f.site}; ${f.kit} is the closest Google Font)` : ""}${f.measured ? "" : " (the parts kit has no metrics for it: sizes are estimated)"}`;
  }),
  ...Object.entries(scale).map(([n, l]: any) => `- ${n}: ${l.size}px ${l.weight ?? ""} ${l.family ?? ""} (site px; the kit scales by 2.5 x frame width / 1080)`),
  kit.button ? `- CTA button: ${kit.button.bg} with ${kit.button.color} text, ${kit.button.font ?? "-"}, radius ${kit.button.radius ?? 0}px` : "",
  "", "## Logos", "",
  ...(kit.logos.length ? kit.logos.map((l) => `- ${l.file} (${l.type}, for ${l.on} backgrounds, ${l.width}x${l.height})`) : ["none"]),
  kit.voice.length ? `\n## Voice\n\n${kit.voice.map((v) => `- ${v}`).join("\n")}` : "",
].filter((l, i, a) => l !== "" || a[i - 1] !== "").join("\n") + "\n";
await Bun.write(`${space}/BRAND.md`, md);

console.log(`wrote BRAND.md, brand/brand.json${sg ? ", brand/styleguide.json, brand/brand.css" : " (no styleguide: colours from the palette only, no fonts or scale)"}`);
console.log(`roles: ${Object.entries(kit.palette).map(([k, v]) => `${k} ${v ?? "none"}`).join(", ")}`);
console.log(`fonts: heading ${kit.fonts.heading?.kit ?? "none"}, body ${kit.fonts.body?.kit ?? "none"}`);
