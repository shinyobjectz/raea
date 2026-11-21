// DR4's proof, the 3D scene as asked: a seat turning, and a rider on it that turns with it and sits forward, clear of
// the back (seat.tsx). checks/rider.ts holds it, with the parts named in taste.ts.
import { Composition, registerRoot, useCurrentFrame } from "raea";
import { Seat } from "./seat";

function Ride() {
  return <Seat yaw={0.3 + useCurrentFrame() * 0.01} rider={[0, 0.35, 0.25]} />;
}

registerRoot(() => <Composition id="ride" component={Ride} width={540} height={960} fps={30} durationInFrames={30} />);
