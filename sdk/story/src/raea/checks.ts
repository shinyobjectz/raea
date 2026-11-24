// A story's checks as one raea/check check (docs/story.feature ST5a). A project uses it explicitly, with a three-line
// check of its own that imports its story:
//
//   // checks/story.ts
//   import { storyCheck } from "raea/story/raea/checks";
//   import launch from "../launch.story";
//   export default storyCheck(launch, { telling: "30s", lenses: ["mckee", "pas", "storynomics"] });
//
// and a taste line that names it: { text: "The story holds and is on screen", check: "checks/story.ts" }.
// On every frame it keeps the shots (a Sequence's name and frames) and the spoken lines; after the last frame it reports
// the story's own findings (exact checks and lenses), then each beat with no shot named for it, a beat whose shot comes
// out of the telling's order, and a beat line that's never spoken inside its shot. It's `over: "both"`: `raea check
// launch.story.ts` runs it with no frames, before anything is drawn, and then only the story's own findings speak.
import { check, found, type Line, type Shot } from "raea/check";
import { judge } from "../sample.ts";
import { LENSES } from "../lenses/index.ts";
import { resolved } from "../load.ts";
import type { Lens } from "../lens.ts";
import type { AnyStory } from "../model.ts";

export type StoryCheckOptions = { telling?: string; seed?: number; lenses?: (string | Lens)[]; onScreen?: boolean; id?: string };

const words = (s: string) => s.toLowerCase().replace(/[^a-z0-9' ]+/g, " ").split(/\s+/).filter(Boolean);

export function storyCheck(s: AnyStory, o: StoryCheckOptions = {}) {
  const lenses = (o.lenses ?? s.lenses ?? []).map((l) => (typeof l === "string" ? LENSES[l] : l)).filter((l): l is Lens => !!l);
  const tellingName = o.telling ?? Object.keys(s.tellings)[0];
  return check(
    { id: o.id ?? "story", text: "the story holds together, and every beat is on screen in order", over: "both" },
    (world, frame, _values, memo) => {
      memo.shots ??= {};
      memo.lines ??= [];
      for (const e of world.find({ with: ["Shot"] })) {
        const sh = world.component<Shot>(e, "Shot")!;
        if (!sh.name) continue;
        const m = (memo.shots[sh.name] ??= { from: sh.from, to: sh.to ?? frame + 1 });
        m.to = Math.max(m.to, sh.to ?? frame + 1);
      }
      for (const e of world.find({ with: ["Line"] })) {
        const l = world.component<Line>(e, "Line")!;
        if (!memo.lines.some((x: Line) => x.from === l.from && x.text === l.text)) memo.lines.push({ text: l.text, from: l.from, to: l.to ?? l.from });
      }
    },
    (_values, memo) => {
      for (const f of judge(s, o.seed ?? 0, tellingName, lenses)) found({ about: [f.about], says: f.says, level: f.level, ...(f.fix ? { fix: f.fix } : {}) });
      // with no frames (raea check NAME.story.ts), the story alone: nothing was drawn to look for
      if (o.onScreen === false || memo.shots === undefined) return;
      const beats = resolved(s, o.seed ?? 0).tellings[tellingName] ?? [];
      let last = -1;
      beats.forEach((b, i) => {
        const key = b.id ?? `b${i}`;
        const shot = memo.shots?.[key] as { from: number; to: number } | undefined;
        if (!shot) return found({ about: [key], says: `the beat ${key} has no shot named for it`, fix: `name the beat's Sequence "${key}" (or use <Beat> from raea/story/raea/compose)` });
        if (shot.from < last) found({ about: [key], frames: [shot.from, shot.to], says: `${key} comes before the beat it follows in the telling`, level: "warn" });
        last = shot.from;
        for (const l of b.says ?? []) {
          const want = words(String(l.says));
          const said = (memo.lines as Line[]).filter((x) => x.from < shot.to && (x.to ?? x.from) >= shot.from).flatMap((x) => words(x.text));
          const heard = want.filter((w) => said.includes(w)).length;
          if (want.length && heard / want.length < 0.6) found({ about: [key], frames: [shot.from, shot.to], says: `${key}'s line "${l.says}" isn't spoken inside its shot`, level: "warn" });
        }
      });
    },
  );
}
