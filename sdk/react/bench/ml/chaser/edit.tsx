// The chaser (game.tsx): a game ("chaser") and a video ("orbit") of a small policy net steering a dot.
import { Composition, registerRoot } from "@raea/motion";
import { Chaser, FPS, H, Orbit, W } from "./game.tsx";

registerRoot(() => (
  <>
    <Composition id="chaser" component={Chaser} width={W} height={H} fps={FPS} durationInFrames={FPS * 60} />
    <Composition id="orbit" component={Orbit} width={W} height={H} fps={FPS} durationInFrames={FPS * 6} />
  </>
));
