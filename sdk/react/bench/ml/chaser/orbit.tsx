// The orbit video alone (game.tsx), for play: play opens an edit's first composition.
import { Composition, registerRoot } from "@raea/motion";
import { FPS, H, Orbit, W } from "./game.tsx";

registerRoot(() => <Composition id="orbit" component={Orbit} width={W} height={H} fps={FPS} durationInFrames={FPS * 6} />);
