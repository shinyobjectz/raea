// The narrator test game (game.tsx): four lines said live, the second cut at CUT_AT with a reaction after it.
import { Composition, registerRoot } from "@raea/motion";
import { FPS, Narrator } from "./game.tsx";

registerRoot(() => <Composition id="narrator" component={() => <Narrator />} width={960} height={540} fps={FPS} durationInFrames={FPS * 40} />);
