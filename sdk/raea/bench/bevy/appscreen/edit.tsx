// <Screen app> (docs/blitz-bevy.feature H12): an app screen, kit.tsx's component on @raea/ui's react-dom in a Blitz
// document of its own, on a flat screen in 3D (320 x 200, 2 units wide, filling x 295-665 and y 154-386 of the frame).
// Its button counts presses (blue "Press", then green "Pressed 1") and rings yellow while the pointer is over it; a pink
// bar along its foot grows on the app's own timer, so the app runs on the frame's clock. check.sh plays it with
// bench/render-play/appscreen.json's pointer and checks the render of the play's log; bench/render-play's row holds
// render = play.
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Screen, ToneMapping } from "raea/3d";
import { Camera3d } from "raea/camera";
import { Transform } from "raea/transform";

function Scene() {
  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <Camera3d transform={Transform.fromXyz(0, 0, 4).lookingAt([0, 0, 0])} projection={{ Perspective: { fov: (40 * Math.PI) / 180 } }} />
      <ToneMapping mode="none" />
      <Screen name="kit" crt={false} resolution={[320, 200]} size={[2]} app="kit.tsx" background="#ffffff" />
    </AbsoluteFill>
  );
}
registerRoot(() => <Composition id="appscreen" component={Scene} width={960} height={540} fps={30} durationInFrames={30} />);
