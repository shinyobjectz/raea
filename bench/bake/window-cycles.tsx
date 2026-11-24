// The window set with Cycles' bounce light baked into its lightmaps by blender/lightbake.py, any sun left live: what raea's
// own bake is measured against (docs/bake.feature BK0). `raea bake light --compare` makes media/baked/window/.
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Baked } from "./baked";
import { Look } from "./parts/window";
import info from "./media/baked/window/set.lightmaps.json";

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Look />
      <Baked dir="baked/window" info={info} />
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="window-cycles" component={Shot} width={1280} height={720} fps={30} durationInFrames={1} />);
