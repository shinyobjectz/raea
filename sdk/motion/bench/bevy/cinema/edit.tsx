// Cinema fixture: a dark toon room lit through a window, with the camera's effects together: volumetric fog shows the
// window's shaft, depth of field pulls focus from the back wall (12) onto the hero (6.1 by frame 45), a grade warms
// the picture and ambient occlusion darkens the contacts, all on throughout; MotionBlur smears the ball rolling across
// from frame 20 (frames 0-19 have no MotionBlur). Each effect's lines are marked {/* fx:NAME */} so cinema/check.py can
// take them out; render equals its baked play (check.sh).
import { AbsoluteFill, Composition, interpolate, registerRoot, useCurrentFrame } from "@raea/motion";
import { AmbientLight, AmbientOcclusion, Box, Canvas3D, DepthOfField, DirectionalLight, FogVolume, Grade, Mesh, MeshToonMaterial, MotionBlur, PerspectiveCamera, Sphere, VolumetricFog, manifold } from "@raea/motion/3d";

const FPS = 30;
const FRAMES = 60;
const WALL = manifold.box(0.3, 6, 14).minus(manifold.box(1, 2.6, 2.4).move(0, 0.6, -0.5));

function Room() {
  const f = useCurrentFrame();
  const focus = interpolate(f, [20, 45], [12, 6.1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill style={{ background: "#05060a" }}>
      <Canvas3D>
        <PerspectiveCamera position={[1.5, 2, 7]} lookAt={[0, 1.8, 0]} fov={45} focus={focus} />
        <AmbientLight intensity={0.15} />
        <DirectionalLight position={[-12, 9, -2]} intensity={2.5} color="#ffe2b0" volumetric />
        <VolumetricFog />{/* fx:vfog */}
        <FogVolume position={[0, 3, 0]} scale={[10, 6.5, 14]} density={0.25} anisotropy={0.8} />{/* fx:vfog */}
        <DepthOfField aperture={0.06} />{/* fx:dof */}
        <Grade temperature={0.3} saturation={1.15} />{/* fx:grade */}
        {f >= 20 ? <MotionBlur shutterAngle={1} /> : null}{/* fx:mblur */}
        <AmbientOcclusion />{/* fx:ao */}
        <Mesh name="wallL" shape={WALL} position={[-4, 3, 0]}><MeshToonMaterial color="#5b6378" steps={2} /></Mesh>
        <Mesh name="wallB" position={[0, 3, -7]}><Box args={[8, 6, 0.3]} /><MeshToonMaterial color="#4a5066" steps={2} /></Mesh>
        <Mesh name="floor" position={[0, -0.15, 0]}><Box args={[8, 0.3, 14]} /><MeshToonMaterial color="#8a6a52" steps={2} /></Mesh>
        <Mesh name="ceil" position={[0, 6.15, 0]}><Box args={[8, 0.3, 14]} /><MeshToonMaterial color="#3a3f50" steps={2} /></Mesh>
        <Mesh name="hero" position={[0.5, 1.1, 1]}><Sphere args={[0.9]} /><MeshToonMaterial color="#ffd23f" steps={2} rim={{ intensity: 0.6, width: 0.2 }} outline={{ width: 3 }} /></Mesh>
        <Mesh name="crate" position={[-1.5, 0.6, -3.5]} rotation={[0, 0.4, 0]}><Box args={[1.2, 1.2, 1.2]} /><MeshToonMaterial color="#c47a3a" steps={2} outline={{ width: 3 }} /></Mesh>
        <Mesh name="ball" position={[3.4 - f * 0.1, 0.5, 3]}><Sphere args={[0.5]} /><MeshToonMaterial color="#3aa0ff" steps={2} outline={{ width: 3 }} /></Mesh>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cinema" component={Room} width={960} height={540} fps={FPS} durationInFrames={FRAMES} />);
