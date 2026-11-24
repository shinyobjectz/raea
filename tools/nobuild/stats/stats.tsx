import { AbsoluteFill, Composition, registerRoot, useCurrentFrame } from "raea";
import { native, print } from "@raea/app/native";
import { Camera3d } from "raea/camera";
import { AmbientLight, DirectionalLight } from "raea/light";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Quat, Transform } from "raea/transform";

// docs/nobuild.feature NB2a: a scene that turns stat channels on from TypeScript and reads the run's numbers back,
// with no variable set and no build (tools/nobuild.sh stats). At its top level, so they're on before the Engine opens:
// bevy_passes (each of Bevy's render passes timed) and trace (a line per update of each frame, RAEA_TRACE's). On its
// last frame it prints what stats.read says so far: the passes timed, and the Engine's draws and updates.
const FRAMES = 12;
native("stats.on", { channels: ["bevy_passes", "trace"] });

function Box() {
  const f = useCurrentFrame();
  if (f === FRAMES - 1) {
    const s = native("stats.read", {});
    const passes = Object.keys(s.passes).filter((k) => k.startsWith("bevy/"));
    print(`stats.read: chosen ${s.chosen.join(",")}; ${passes.length} bevy passes timed; draws ${s.counters["engine/draws"] ?? 0}; updates ${s.counters["engine/updates"] ?? 0}`);
  }
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <>
        <Camera3d transform={Transform.fromXyz(0, 1.5, 5).lookingAt([0, 0.5, 0])} projection={{ Perspective: { fov: 0.785398163397448 } }} />
        <AmbientLight brightness={200} />
        <DirectionalLight transform={Transform.fromXyz(3, 6, 4).lookingAt([0, 0, 0])} illuminance={10400} shadow_maps_enabled />
        <Mesh3d name="floor">
          <Cuboid half_size={[3, 0.1, 2]} />
          <StandardMaterial base_color="#334155" perceptual_roughness={0.8} />
        </Mesh3d>
        <Mesh3d transform={Transform.fromXyz(0, 0.6, 0).withRotation(Quat.fromEuler("XYZ", 0, f * 0.1, 0))} name="box">
          <Cuboid half_size={[0.5, 0.5, 0.5]} />
          <StandardMaterial base_color="#e11d48" perceptual_roughness={0.4} />
        </Mesh3d>
      </>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="stats" component={Box} width={320} height={180} fps={30} durationInFrames={FRAMES} />);
