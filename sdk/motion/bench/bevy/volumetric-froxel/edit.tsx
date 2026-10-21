// Volumetric fixture on the froxel grid: the volumetric fixture with <VolumetricFog mode="froxel"> and no ambient
// (the grid scatters ambient light by the fog's density where Bevy's march adds it over each whole box, so with it the
// two differ on purpose). A spot light shafting through a slot in a wall and a low sun, both volumetric, inside a
// <FogVolume> that drifts, thickens and turns red over the shot; a second volume, smoke, appears at frame 30 from a
// Sequence. fog-score.py scores it against the march at 1024 steps.
import { AbsoluteFill, Composition, interpolate, registerRoot, Sequence, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, FogVolume, Mesh, MeshStandardMaterial, PerspectiveCamera, SpotLight, Sphere, VolumetricFog, manifold } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;

// a wall with a slot in it
const wall = manifold.box(8, 5, 0.3).minus(manifold.box(0.6, 2, 1).move(-1, 0.8, 0));

function Shot() {
  const f = useCurrentFrame();
  const drift = interpolate(f, [0, FRAMES - 1], [-0.5, 0.5]);
  const thick = interpolate(f, [0, FRAMES - 1], [0.08, 0.2]);
  const red = f < FRAMES / 2 ? "#fff0e0" : "#c0281c";
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Canvas3D>
        <PerspectiveCamera position={[4.5, 2, 5]} lookAt={[-0.5, 1.5, -1]} fov={50} />
        <VolumetricFog mode="froxel" />
        <AmbientLight intensity={0.1} />
        <DirectionalLight position={[3, 2, -4]} intensity={0.15} color="#ffb070" volumetric />
        <SpotLight position={[-2.5, 6, -6]} target={[0, 0.5, 0.5]} angle={0.3} penumbra={0.2} intensity={2.5} color="#ffe6c4" volumetric />
        <Mesh position={[0, -0.05, 0]}>
          <Box args={[10, 0.1, 10]} />
          <MeshStandardMaterial color="#6a6258" roughness={0.9} />
        </Mesh>
        <Mesh name="wall" shape={wall} position={[0, 2.5, -2.5]}>
          <MeshStandardMaterial color="#5a5248" roughness={0.9} />
        </Mesh>
        <Mesh position={[0, 0.6, 0.5]}>
          <Sphere args={[0.6]} />
          <MeshStandardMaterial color="#d8d0c4" roughness={0.6} />
        </Mesh>
        <FogVolume name="air" position={[drift, 2, 0.5]} scale={[8, 4, 6]} density={thick} color={red} scattering={0.45} absorption={0.15} anisotropy={0.6} />
        <Sequence from={30} layout="none">
          <FogVolume name="smoke" position={[2, 0.6, 1.5]} scale={[1.5, 1.2, 1.5]} density={0.5} color="#404040" absorption={0.6} scattering={0.2} />
        </Sequence>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="volumetric-froxel" component={Shot} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
