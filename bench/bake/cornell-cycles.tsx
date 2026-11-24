// The cornell set with Cycles' bounce light baked into its lightmaps by blender/lightbake.py, any sun left live: what raea's
// own bake is measured against (docs/bake.feature BK0). `raea bake light --compare` makes media/baked/cornell/.
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Baked } from "./baked";
import { Look } from "./parts/cornell";
import info from "./media/baked/cornell/set.lightmaps.json";

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Look />
      <Baked dir="baked/cornell" info={info} />
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cornell-cycles" component={Shot} width={1280} height={720} fps={30} durationInFrames={1} />);
