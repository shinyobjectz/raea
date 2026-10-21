// The narrator test game with the narrator silent: the same page and fold without speech, for frame times without it.
import { Composition, registerRoot } from "@raea/motion";
import { FPS, Narrator } from "./game.tsx";

registerRoot(() => <Composition id="quiet" component={() => <Narrator quiet />} width={960} height={540} fps={FPS} durationInFrames={FPS * 40} />);
