// UGC Reel parts as React components: props in, rrl rows out. Each part's pieces (a line, a word, a bar) are their own
// rows, so rrl keeps per-frame motion and a later move can tune any one of them. Cues are phrases from the take.
// Every colour, face and size comes from the brand (useTheme: the space's brand/brand.json, the edit row, or the
// part's props: accent, surface, text_color, accent_text, display_font, text_font, type_scale); every position from
// the frame (useFrame: x and sizes in 1080-wide units, y in px or as a fraction of the height). A part missing a
// value it needs throws, naming the prop to pass.
import { Fragment } from "react";
import { k } from "./rows";
import { useEdit, useTheme, useFrame, cue, fit, measure, plain, alpha, halo } from "./kit";

const pop = k({ 0: 0.6, 4: 1.08, 8: 1 }, "ease out local");
const rise = (y: number, d = 30) => k({ 0: y + d, 6: y }, "ease out local");
const display = (font: string, weight: number, size: number, color: string, tracking: number) =>
  ({ font, weight, size, color, align: "center", line: 0.92, case: "uppercase", tracking, textShadow: halo(color) });

type Placed = { id: string; from: number; to: number };
type Themed = { accent?: string; surface?: string; textColor?: string; accentText?: string; displayFont?: string; textFont?: string; typeScale?: number };

/** The lines of a lockup with their sizes and tops (Lockup draws them; NotThat sets its card below them). */
function lockupLines(text: string, top: number, font: string, weight: number, hotMax: number, small: number, w: number, tracking: number, gap: number) {
  let y = top;
  return text.split(" / ").map((line) => {
    const hot = line.startsWith("*"), t = plain(line);
    const size = fit(t, font, hot ? hotMax : small, w, tracking, true, weight);
    const at = y;
    y += Math.round(size * 0.92) + gap;
    return { hot, t, size, y: at, bottom: y };
  });
}

/** Lines stacked big, each landing as it's said: "*first line* / second / third" (*hot* = the accent, pops). */
export function Lockup(p: Placed & Themed & { text: string; y?: number; max?: number; small?: number; x?: number; w?: number }) {
  const { words } = useEdit();
  const { need, level } = useTheme("lockup", p);
  const { u, Y, X } = useFrame();
  const font = need("displayFont"), color = need("textColor"), hotColor = need("accent");
  const h1 = level("h1", p.max), h2 = level("h2", p.small);
  const weight = h1.weight, tracking = h1.tracking;
  const { id, from, to } = p, x = X(p.x ?? 90 * u), w = Math.round(p.w ?? 900 * u);
  const lines = lockupLines(p.text, Y(p.y ?? 0.12), font, weight, h1.size, h2.size, w, tracking, Math.round(14 * u));
  return (
    <>
      {lines.map((l, i) => {
        const c = i === 0 ? from : Math.max(from, cue(words, l.t.split(" ")[0], from, to) ?? from);
        const at = c - from < 20 ? from : c;
        return (
          <title key={i} id={`${id}_l${i}`} text={l.t} {...display(font, weight, l.size, l.hot ? hotColor : color, tracking)} x={x} w={w} y={l.y}
            from={at} to={to} anim={l.hot ? { scale: pop } : { y: rise(l.y, 30 * u) }} />
        );
      })}
    </>
  );
}

/** Word-by-word captions for what's said in [from, to): 1-3 words a line, the spoken word lit in the accent; `keys` stay lit. */
export function Captions(p: Placed & Themed & { keys?: string; y?: number; max?: number; lines?: string; fix?: string }) {
  const { id, from, to, keys = "", lines, fix = "" } = p;
  const { need, level } = useTheme("captions", p);
  const { u, Y } = useFrame();
  const font = need("displayFont"), color = need("textColor"), lit = need("accent");
  const h2 = level("h2", p.max), max = h2.size, y = Y(p.y ?? 0.135);
  const x = Math.round(70 * u), w = Math.round(940 * u);
  const fixes = Object.fromEntries(fix.split(",").filter(Boolean).map((f) => f.split("=")));
  const words = useEdit().words.map((w) => ({ ...w, text: fixes[w.text] ?? w.text }));
  const hot = new Set(keys.split(",").map((s) => s.trim().toLowerCase()).filter(Boolean));
  const ws = words.filter((w) => w.ws >= from && w.ws < to);
  const groups: typeof ws[] = [];
  const sizes = lines?.split(" / ").map((l) => l.trim().split(/\s+/).length);
  for (const w of ws) {
    const g = groups[groups.length - 1];
    if (sizes) { if (!g || g.length >= sizes[groups.length - 1]) groups.push([w]); else g.push(w); continue; }
    const prev = g?.[g.length - 1];
    if (!g || g.length >= 3 || /[.,?!]$/.test(prev!.text) || w.ws - prev!.we > 8) groups.push([w]);
    else g.push(w);
  }
  return (
    <>
      <rule label="karaoke">W.color = {lit} if text W {"{"}class: "w"{"}"}, frame &gt;= W.ws, frame &lt; W.we</rule>
      <rule label="key-lit">W.color = {lit} if text W {"{"}class: "k"{"}"}, frame &gt;= W.ws</rule>
      {groups.map((g, i) => {
        const gf = g[0].ws, gt = Math.min(to, groups[i + 1]?.[0].ws ?? g[g.length - 1].we + 8);
        const size = fit(g.map((w) => w.text).join(" "), font, max, w, h2.tracking, true, h2.weight);
        return (
          <title key={i} id={`${id}_g${i}`} text="" {...display(font, h2.weight, size, color, h2.tracking)} line={0.95} x={x} w={w} y={y} from={gf} to={gt}
            anim={{ scale: k({ 0: 0.8, 3: 1.06, 6: 1 }, "ease out local"), y: k({ 0: y + Math.round(26 * u), 5: y }, "ease out local") }}>
            {g.map((w, j) => (
              <text key={j} id={`${id}_g${i}w${j}`} text={w.text + (j < g.length - 1 ? " " : "")}
                ws={w.ws} we={Math.max(w.we, w.ws + 4)} class={hot.has(w.text.toLowerCase().replace(/[^a-z0-9%']/g, "")) ? "k" : "w"} />
            ))}
          </title>
        );
      })}
    </>
  );
}

/** A card in the brand's surface: a big number, three lines beside it, and two bars that fill as the shares are said.
 *  `swap` continues a card already on screen (no entrance; bars start from p1/p2). */
export function StatCard(p: Placed & Themed & {
  big: string; bigSay?: string; l1: string; l2: string; l3: string; l3Say?: string;
  bar1?: string; v1?: number; v1Label?: string; v1Say?: string; bar2?: string; v2?: number; v2Label?: string; v2Say?: string;
  p1?: number; p2?: number; p1Label?: string; p2Label?: string; swap?: number; y?: number; max?: number; lineMax?: number; valueSize?: number; labelSize?: number; radius?: number;
}) {
  const { words } = useEdit();
  const { t, need, level, typeScale } = useTheme("statcard", p);
  const { u, Y } = useFrame();
  const font = need("displayFont"), textFont = need("textFont"), accent = need("accent"), surface = need("surface"), ink = need("textColor");
  const h1 = level("h1", p.max), h3 = level("h3", p.lineMax), h4 = level("h4", p.valueSize), body = level("p", p.labelSize);
  const W = Math.round(960 * u), PAD = Math.round(50 * u), TRACK = W - 2 * PAD;
  // the stat is the card's display moment: twice the heading size, fitted to half the card
  const bigSize = fit(p.big, font, p.max ?? 2 * h1.size, Math.round(480 * u), h1.tracking, true, h1.weight);
  const bigW = measure(p.big, font, bigSize, h1.tracking, true, h1.weight);
  const lx = PAD + Math.round(bigW) + Math.round(18 * u), lw = W - PAD - lx;
  const lineSize = Math.min(...[p.l1, p.l2, p.l3].map((l) => fit(plain(l), font, h3.size, lw, h3.tracking, true, h3.weight)));
  // only the bars it's given (a card with a number and three lines has none), and a card the height of what's in it
  const bars = ([1, 2] as const).filter((n) => (n === 1 ? p.bar1 : p.bar2) != null && (n === 1 ? p.v1 : p.v2) != null);
  const lineTop = Math.round(44 * u), barTop = Math.max(Math.round(bigSize * 0.95) + Math.round(40 * u), lineTop + 3 * Math.round(lineSize * 0.97) + Math.round(24 * u));
  const barH = Math.round(40 * u), barStep = body.size + barH + Math.round(40 * u);
  const top = barTop + Math.round(16 * u);
  const { id, from, to } = p;
  const y = Y(p.y ?? 0.123), h = (p as any).h ?? (bars.length ? barTop + barStep * bars.length + Math.round(16 * u) : top);
  const radius = Math.round(p.radius ?? (t.radius != null ? t.radius * typeScale : 0));
  const lines = [p.l1, p.l2, p.l3];
  const lineAt = (i: number) => i === 2 && p.l3Say ? cue(words, p.l3Say, from, to)! : from + 2 + 2 * i;
  const fillW = (v: number) => Math.max(Math.round(18 * u), Math.round(TRACK * v / 100));
  const bar = (n: 1 | 2) => {
    const v = (n === 1 ? p.v1 : p.v2) ?? 0, say = n === 1 ? p.v1Say : p.v2Say, start = (n === 1 ? p.p1 : p.p2) ?? 0;
    const at = say ? cue(words, say, from, to)! : from + 8;
    const by = barTop + barStep * (bars.indexOf(n));
    const ty = by + body.size + Math.round(8 * u);
    return (
      <>
        <title id={`${id}_lb${n}`} text={n === 1 ? p.bar1 : p.bar2} font={textFont} weight={body.weight} size={body.size} color={alpha(ink, 0.85)}
          x={PAD} w={Math.round(600 * u)} y={by} align="left" tracking={body.tracking} line={1} case="uppercase" />
        <box id={`${id}_tr${n}`} x={PAD} y={ty} w={TRACK} h={barH} bg={alpha(ink, 0.16)} radius={Math.round(barH / 2)} />
        <box id={`${id}_fi${n}`} x={PAD} y={ty} w={fillW(start)} h={barH} bg={accent} radius={Math.round(barH / 2)}
          anim={{ w: k({ [at]: fillW(start), [at + 24]: fillW(v) }, "ease out") }} />
        <title id={`${id}_vl${n}`} text={(n === 1 ? p.v1Label : p.v2Label) ?? `${v}%`} font={font} weight={h4.weight} size={h4.size}
          color={accent} x={W - PAD - Math.round(300 * u)} w={Math.round(300 * u)} y={by - Math.round(4 * u)} align="right" line={1} from={at} to={to} />
        {(n === 1 ? p.p1Label : p.p2Label) && at > from ? (
          <title id={`${id}_pl${n}`} text={(n === 1 ? p.p1Label : p.p2Label)!} font={font} weight={h4.weight} size={h4.size} color={alpha(ink, 0.5)}
            x={W - PAD - Math.round(300 * u)} w={Math.round(300 * u)} y={by - Math.round(4 * u)} align="right" line={1} from={from} to={at} />) : null}
      </>
    );
  };
  return (
    <box id={id} x={Math.round(60 * u)} y={y} w={W} h={h} bg={alpha(surface, 0.92)} radius={radius} shadow={t.shadow}
      from={from} to={to} anim={p.swap ? { scale: k({ 0: 1, 3: 1.035, 9: 1 }, "ease out local") } : { y: k({ 0: y - Math.round(30 * u), 7: y }, "ease out local") }}>
      <title id={`${id}_big`} text={p.big} {...display(font, h1.weight, bigSize, accent, h1.tracking)} textShadow={undefined} align="left" x={PAD} w={Math.round(bigW) + 10} y={Math.round(20 * u)}
        anim={p.bigSay ? { scale: k({ [cue(words, p.bigSay, from, to)!]: 1, [cue(words, p.bigSay, from, to)! + 3]: 1.12, [cue(words, p.bigSay, from, to)! + 8]: 1 }, "ease out") } : { scale: pop }} />
      {lines.map((l, i) => {
        const at = lineAt(i), ly = lineTop + i * Math.round(lineSize * 0.97);
        return (
          <title key={i} id={`${id}_t${i}`} text={plain(l)} {...display(font, h3.weight, lineSize, l.startsWith("*") ? accent : i === 1 ? alpha(ink, 0.72) : ink, h3.tracking)}
            textShadow={undefined} align="left" x={lx} w={lw} y={ly} from={at} to={to} anim={{ y: rise(ly, 30 * u) }} />
        );
      })}
      {bars.map((n) => bar(n))}
    </box>
  );
}

/** "this, not that": a lead lockup, then the old way on a card, struck through once it's said. */
export function NotThat(p: Placed & Themed & { lead: string; not: string; strikeSay: string; y?: number; strikeColor?: string; radius?: number; max?: number; lineMax?: number }) {
  const { id, from, to, lead, not, strikeSay } = p;
  const { words } = useEdit();
  const { t, need, level, typeScale } = useTheme("notthat", p);
  const { u, Y } = useFrame();
  const font = need("displayFont"), surface = need("surface"), ink = need("textColor");
  const strikeColor = p.strikeColor ?? need("accent");
  const h1 = level("h1", p.max), h2 = level("h2", p.max), h3 = level("h3", p.lineMax);
  const y = Y(p.y ?? 0.123), lw = Math.round(900 * u);
  // the card sits under the lead, wherever its lines end on this frame
  const leadLines = lockupLines(lead, y, font, h1.weight, h2.size, h2.size, lw, h1.tracking, Math.round(14 * u));
  const cy = leadLines[leadLines.length - 1].bottom + Math.round(24 * u);
  const cx = Math.round(100 * u), cw = Math.round(880 * u), inner = Math.round(820 * u);
  const step = Math.round(h3.size * 1.15), pad = Math.round(24 * u);
  const lines = not.split(" / ");
  const cardAt = cue(words, lines[0].split(" ")[0], from, to)!;
  const strike = Math.min(to - 12, cue(words, strikeSay, from, to)!);
  const radius = Math.round(p.radius ?? (t.radius != null ? t.radius * typeScale : 0));
  return (
    <>
      <Lockup {...p} id={`${id}_lead`} from={from} to={to} text={lead} y={y} max={h2.size} small={h2.size} />
      <box id={id} x={cx} y={cy} w={cw} h={2 * pad + step} bg={alpha(surface, 0.9)} radius={radius} shadow={t.shadow} from={cardAt} to={to}
        anim={{ y: k({ 0: cy + Math.round(30 * u), 6: cy }, "ease out local"), h: k(Object.fromEntries(lines.flatMap((l, i) => {
          const at = Math.max(cardAt, cue(words, l.split(" ")[0], from, to)!);
          return i === 0 ? [[at, 2 * pad + step]] : [[at - 1, 2 * pad + step * i], [at + 5, 2 * pad + step * (i + 1)]];
        })), "ease out") }}>
        {lines.map((l, i) => {
          const size = fit(l, font, h3.size, inner, h3.tracking, true, h3.weight), tw = Math.min(inner, measure(l, font, size, h3.tracking, true, h3.weight));
          const at = Math.max(cardAt, cue(words, l.split(" ")[0], from, to)!);
          const ly = pad + i * step;
          return (
            <Fragment key={i}>
              <title key={`t${i}`} id={`${id}_n${i}`} text={l} {...display(font, h3.weight, size, ink, h3.tracking)} textShadow={undefined} x={Math.round(20 * u)} w={cw - Math.round(40 * u)}
                y={ly} from={at} to={to} anim={{ y: rise(ly, 30 * u) }} />
              <box key={`s${i}`} id={`${id}_x${i}`} x={Math.round(cw / 2 - tw / 2)} y={Math.round(ly + size * 0.42)} w={0} h={Math.max(4, Math.round(size * 0.12))} bg={strikeColor} radius={Math.round(size * 0.06)}
                from={strike + 4 * i} to={to} anim={{ w: k({ 0: 0, 8: Math.round(tw) }, "ease out local") }} />
            </Fragment>
          );
        })}
      </box>
    </>
  );
}

/** A "+ FOLLOW" pill that hugs its text (measured, so it never clips), styled as the site's primary button. */
export function Follow(p: Placed & Themed & { text?: string; y?: number; size?: number }) {
  const { id, from, to, text = "+ follow" } = p;
  const { t, need, level, typeScale } = useTheme("follow", p);
  const { u, Y, width } = useFrame();
  const btn = t.button ?? {};
  // the brand's button when it has one; else the accent with the text colour picked for it
  const bg = p.accent ?? btn.bg ?? need("accent"), color = p.accentText ?? btn.color ?? need("accentText");
  const font = p.textFont ?? btn.font ?? need("textFont");
  const h3 = level("h3", p.size);
  const size = h3.size, weight = btn.weight ?? h3.weight;
  const padX = Math.round(56 * u), padY = Math.round(size * 0.4);
  const w = Math.round(measure(text, font, size, h3.tracking, true, weight)) + 2 * padX;
  const radius = btn.radius != null ? Math.round(btn.radius * typeScale) : Math.round(size);
  return (
    <title id={id} text={text} font={font} weight={weight} size={size} color={color} bg={bg} padding={padY} radius={radius}
      x={Math.round(width / 2 - w / 2)} w={w - 2 * padY} y={Y(p.y ?? 0.78)} align="center" tracking={h3.tracking} line={1} case="uppercase" from={from} to={to}
      shadow={t.shadow} anim={{ scale: pop }} />
  );
}

export const registry = { lockup: Lockup, captions: Captions, statcard: StatCard, notthat: NotThat, follow: Follow };
