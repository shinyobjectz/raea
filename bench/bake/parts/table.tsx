// The table set (docs/bake.feature BK0): sdk/guides/craft/bevy/realism.md's example, flat colours, where
// docs/blender-research.md measured 0.154 / 0.074 / 0.040 against Cycles. Look is everything the bake leaves live (the
// camera, sky, sun and finish); Parts is the static set a bake covers, every part named so its lightmap finds it.
import { staticFile } from "raea";
import { Environment, Sphere, ToneMapping, Bloom, Grade, AmbientOcclusion, Shadows, DepthOfField } from "raea/3d";
import { StandardMaterial } from "raea/pbr";
import { Cuboid, Cylinder, Mesh3d } from "raea/mesh";
import { DirectionalLight } from "raea/light";
import { Transform } from "raea/transform";
import { Camera3d } from "raea/camera";

// what look.py is given for the Cycles reference: these same values (apps/lightbake/sets.json)
export const SKY = "kloofendal_48d_partly_cloudy_puresky_1k.hdr";

export function Look() {
  return (
    <>
      <Camera3d transform={Transform.fromXyz(0.9, 1.35, 2.3).lookingAt([0, 0.85, 0])} projection={{ Perspective: { fov: (38 * Math.PI) / 180 } }}>
        <DepthOfField focal_distance={2.5} aperture_f_stops={2.8} />
      </Camera3d>
      <Environment src={staticFile(SKY)} intensity={0.25} rotation={0.6} background />
      <DirectionalLight transform={Transform.fromXyz(4, 5, 3).lookingAt([0, 0, 0])} illuminance={45000} color="#fff1e0" shadow_maps_enabled />
      <Shadows distance={8} firstCascade={3} mapSize={4096} />
      <AmbientOcclusion quality="high" />
      <ToneMapping mode="agx" exposure={-0.8} />
      <Bloom intensity={0.08} />
      <Grade saturation={1.05} vignette={{ intensity: 0.35, radius: 0.8, smoothness: 3 }} aberration={0.003} />
    </>
  );
}

const LEGS: [number, number][] = [[-0.74, -0.4], [0.74, -0.4], [-0.74, 0.4], [0.74, 0.4]];

export function Parts() {
  return (
    <>
      <Mesh3d name="floor" transform={Transform.fromXyz(0, -0.05, 0)}>
        <Cuboid half_size={[6, 0.05, 6]} />
        <StandardMaterial base_color="#9a958c" perceptual_roughness={1} />
      </Mesh3d>
      <Mesh3d name="wall" transform={Transform.fromXyz(0, 1.5, -1.6)}>
        <Cuboid half_size={[6, 1.5, 0.08]} />
        <StandardMaterial base_color="#9a958c" perceptual_roughness={1} />
      </Mesh3d>
      <Mesh3d name="top" transform={Transform.fromXyz(0, 0.74, 0)}>
        <Cuboid half_size={[0.8, 0.025, 0.45]} />
        <StandardMaterial base_color="#7a3e22" perceptual_roughness={0.45} />
      </Mesh3d>
      {LEGS.map(([x, z], i) => (
        <Mesh3d key={`leg${i}`} name={`leg${i}`} transform={Transform.fromXyz(x, 0.36, z)}>
          <Cuboid half_size={[0.03, 0.36, 0.03]} />
          <StandardMaterial base_color="#7a3e22" perceptual_roughness={0.55} />
        </Mesh3d>
      ))}
      <Mesh3d name="ball" transform={Transform.fromXyz(-0.35, 0.925, 0.05)}>
        <Sphere radius={0.16} />
        <StandardMaterial base_color="#ffffff" perceptual_roughness={0.06} metallic={1} />
      </Mesh3d>
      <Mesh3d name="vase" transform={Transform.fromXyz(0.3, 0.965, -0.1)}>
        <Cylinder radius={0.09} half_height={0.2} />
        <StandardMaterial base_color="#c9b9a3" perceptual_roughness={0.3} />
      </Mesh3d>
    </>
  );
}
