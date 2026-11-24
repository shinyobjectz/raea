import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Camera3d } from "raea/camera";
import { AmbientLight, DirectionalLight } from "raea/light";
import { ConicalFrustum, Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { ColliderConstructor, Restitution, RigidBody } from "raea/plugins/avian";
import { Particles } from "raea/plugins/hanabi";
import { Sphere } from "raea/3d";
import { Transform } from "raea/transform";

// docs/nobuild.feature NB4a: a project choosing avian and hanabi from the catalog (package.json raea.plugins beside
// it), rendered and played by the release raea (tools/nobuild.sh catalog). A ball dropped on a floor (avian) and a
// fountain of orange GPU particles beside it (hanabi). By frame 40 the ball has fallen from the top of the frame to
// the floor, and the fountain fills a column over the pedestal on the right.
function Drop() {
  return (
    <AbsoluteFill style={{ background: "#0f1720" }}>
      <>
        <Camera3d transform={Transform.fromXyz(0, 2, 7).lookingAt([0, 1.2, 0])} projection={{ Perspective: { fov: 0.785398163397448 } }} />
        <AmbientLight brightness={200} />
        <DirectionalLight transform={Transform.fromXyz(3, 6, 4).lookingAt([0, 0, 0])} illuminance={10400} shadow_maps_enabled />
        <Mesh3d name="floor">
          <Cuboid half_size={[4, 0.15, 2]} />
          <StandardMaterial base_color="#334155" perceptual_roughness={0.8} />
          <RigidBody value="Static" />
          <ColliderConstructor value={{ Cuboid: { x_length: 8, y_length: 0.3, z_length: 4 } }} />
        </Mesh3d>
        <Mesh3d transform={Transform.fromXyz(-1.2, 3.2, 0)} name="ball" driven={true}>
          <Sphere radius={0.4} />
          <StandardMaterial base_color="#e11d48" perceptual_roughness={0.4} />
          <RigidBody value="Dynamic" />
          <ColliderConstructor value={{ Sphere: { radius: 0.4 } }} />
          <Restitution value={{ coefficient: 0.3, combine_rule: "Min" }} />
        </Mesh3d>
        <Mesh3d transform={Transform.fromXyz(1.8, 0.3, 0)} name="pedestal">
          <ConicalFrustum radius_top={0.25} radius_bottom={0.35} height={0.3} />
          <StandardMaterial base_color="#111111" />
          <Particles value={{ rate: 2000, lifetime: 1.2, speed: 3, spread: 0.15, size: 0.08, color: [1, 0.55, 0.1, 1], gravity: 4, seed: 7, opaque: true }} />
        </Mesh3d>
      </>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="drop" component={Drop} width={480} height={270} fps={30} durationInFrames={48} />);
