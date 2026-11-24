// The window set drawn by raea with nothing baked: the "before" frame (docs/bake.feature BK0, apps/lightbake/sets.json).
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Look, Parts } from "./parts/window";

function Shot() {
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Look />
      <Parts />
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="window" component={Shot} width={1280} height={720} fps={30} durationInFrames={1} />);
