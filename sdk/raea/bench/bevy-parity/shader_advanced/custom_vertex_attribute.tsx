// Bevy v0.20.0 examples/shader_advanced/custom_vertex_attribute.rs in raea's TSX (docs/control.feature FC2d): a
// default Cuboid given its own vertex attribute, ATTRIBUTE_BLEND_COLOR ("BlendColor", id 988540917, Float32x4), red
// at all 24 vertices, drawn by a Material whose vertex and fragment shaders are assets/shaders/custom_vertex_attribute.wesl
// as written; its specialize lays the vertex buffer out as POSITION at location 0 and BlendColor at 1, and its
// uniform `material` is { color: LinearRgba::WHITE }. The cube is at (0, 0.5, 0), the camera at (-2, 2.5, 5) looking
// at the origin, with no light, in Bevy's default 1280 x 720 window.
import { Composition, registerRoot, staticFile } from "raea";
import { Camera3d } from "raea/camera";
import { ATTRIBUTE_POSITION, Cuboid, Mesh3d, MeshVertexAttribute } from "raea/mesh";
import { Material } from "@raea/render/bevy3d"; // raea/pbr once scene-loop lands (its pbr.ts commit)
import { Transform } from "raea/transform";

const ATTRIBUTE_BLEND_COLOR = MeshVertexAttribute.new("BlendColor", 988540917, "Float32x4");
const SHADER = staticFile("shaders/custom_vertex_attribute.wesl");

function Shot() {
  return (
    <>
      <Mesh3d transform={Transform.fromXyz(0, 0.5, 0)} attributes={[[ATTRIBUTE_BLEND_COLOR, Array.from({ length: 24 }, () => [1, 0, 0, 1])]]}>
        <Cuboid />
        <Material vertex={SHADER} fragment={SHADER} vertex_attributes={[[ATTRIBUTE_POSITION, 0], [ATTRIBUTE_BLEND_COLOR, 1]]} uniforms={{ material: { color: [1, 1, 1, 1] } }} />
      </Mesh3d>
      <Camera3d transform={Transform.fromXyz(-2, 2.5, 5).lookingAt([0, 0, 0])} />
    </>
  );
}

registerRoot(() => <Composition id="main" component={Shot} durationInFrames={11} fps={60} width={1280} height={720} />);
