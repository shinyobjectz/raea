import { Composition, registerRoot, useCurrentFrame } from "raea";
import { AntiAliasing, type AntiAliasingProps } from "raea/post_process";
import { Camera3d } from "raea/camera";
import { DirectionalLight } from "raea/light";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Transform } from "raea/transform";

// docs/nobuild.feature NB1a: <AntiAliasing mode="steady" steady> against RAEA_STEADY and RAEA_STEADY_* (tools/nobuild.sh
// props). Thin bars receding into the distance (detail a pixel wide, which steady settles) and a block sliding past
// (motion, which it follows).
const MODES: Record<string, AntiAliasingProps | undefined> = {
  plain: undefined,
  steady: { mode: "steady" },
  rates: { mode: "steady", steady: { still: 8, moving: 2, px: 1 } },
};

function Scene({ aa }: { aa?: AntiAliasingProps }) {
  const f = useCurrentFrame();
  return (
    <>
      <Camera3d transform={Transform.fromXyz(0, 1.2, 6).lookingAt([0, 0.6, -10])} />
      {aa && <AntiAliasing {...aa} />}
      <DirectionalLight illuminance={6000} transform={Transform.fromXyz(3, 5, 2).lookingAt([0, 0, 0])} />
      <Mesh3d><Cuboid half_size={[6, 0.02, 30]} /><StandardMaterial base_color="#3a3d44" /></Mesh3d>
      {Array.from({ length: 24 }, (_, i) => (
        <Mesh3d key={i} transform={Transform.fromXyz(-3 + (i % 2) * 6, 0.6, 4 - i * 1.6)}>
          <Cuboid half_size={[0.015, 0.6, 0.015]} />
          <StandardMaterial base_color="#f0e8d8" />
        </Mesh3d>
      ))}
      <Mesh3d transform={Transform.fromXyz(-2.5 + f * 0.4, 0.5, 1)}><Cuboid half_size={[0.5, 0.5, 0.5]} /><StandardMaterial base_color="#c0392b" /></Mesh3d>
    </>
  );
}

registerRoot(() => (
  <>
    {Object.entries(MODES).map(([id, aa]) => (
      <Composition key={id} id={id} component={() => <Scene aa={aa} />} width={320} height={180} fps={30} durationInFrames={12} />
    ))}
  </>
));
