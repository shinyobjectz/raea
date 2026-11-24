// Tension (docs/story.feature ST1a): at each beat, from what the audience is waiting on and how much is at stake.
//   open questions (mystery), capped at 3        weight 0.4
//   suspense (facts an agent lacks), capped at 3 weight 0.3
//   pressure: how near the idea's value (else every value, averaged) is to its lowest in this telling, 0 to 1   weight 0.3
// Beats get "Tension" (0 to 1); the world gets "TensionCurve". Needs values and knowledge first.
import type { World } from "../ecs.ts";
import { beatsOf, infoOf } from "../load.ts";
import type { Audience } from "./know.ts";
import type { Curve } from "./values.ts";

export function tension(w: World): void {
  const curves = w.resource<Record<string, Curve>>("Curves") ?? {};
  const focus = infoOf(w).idea?.value;
  const names = focus && curves[focus] ? [focus] : Object.keys(curves);
  const curve: number[] = [];
  for (const { e, index } of beatsOf(w)) {
    const a = w.must<Audience>(e, "Audience");
    const pressure = names.length
      ? names.reduce((sum, n) => {
          const l = curves[n].levels.slice(1);
          const lo = Math.min(...l), hi = Math.max(...l);
          return sum + (hi === lo ? 0 : (hi - l[index]) / (hi - lo));
        }, 0) / names.length
      : 0;
    const t = 0.4 * (Math.min(a.open.length, 3) / 3) + 0.3 * (Math.min(a.suspense.length, 3) / 3) + 0.3 * pressure;
    const r = Math.round(t * 1000) / 1000;
    w.insert(e, "Tension", r);
    curve.push(r);
  }
  w.setResource<number[]>("TensionCurve", curve);
}
