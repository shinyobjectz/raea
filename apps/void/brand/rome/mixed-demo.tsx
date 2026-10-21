// mixed.tsx's pieces, each in a small scene lit as the film is (agx, bloom, fog): `fire` (the pyre from footage, and
// footage flames on a stack built in 3D), `air` (beams and motes in a dark hall, cool haze), `sand` (a drawing in sand
// over a 3D scene). Render a still of each:
//   raea-motion render mixed-demo.tsx --composition fire --at 1,2.5 -o DIR --w 960 --h 540
import { AbsoluteFill, Audio, Composition, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Bloom, Box, Canvas3D, DirectionalLight, Fog, Mesh, MeshStandardMaterial, Model, PerspectiveCamera, Plane, SpotLight, ToneMapping } from "@raea/motion/3d";
import { sao } from "@raea/motion/tone";
import { DustShafts, Footage, Haze, SandDrawing } from "./mixed";

type V = [number, number, number];
const FPS = 30;

function Ground({ name, color = "#3b3228" }: { name: string; color?: string }) {
  return (
    <Mesh name={name} rotation={[-Math.PI / 2, 0, 0]}>
      <Plane args={[200, 200]} />
      <MeshStandardMaterial color={color} roughness={0.95} />
    </Mesh>
  );
}

/** each demo's sound bed (SAO, local): the render refuses silence in a folder whose edits write <Audio> */
const Bed = ({ prompt }: { prompt: string }) => <Audio src={sao(prompt, { seconds: 10 })} gainDb={-12} fadeIn={0.5} />;

function Grade() {
  return (
    <>
      <ToneMapping mode="agx" exposure={1.0} />
      <Bloom intensity={0.22} threshold={0.92} />
      <Fog color="#0c0907" near={22} far={110} />
    </>
  );
}

function Fire() {
  const t = useCurrentFrame() / FPS;
  const cam: V = [Math.sin(t * 0.15) * 3, 1.7, 10.5];
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Bed prompt="a large bonfire crackling and roaring outdoors at night, close, no music" />
      <Canvas3D>
        <PerspectiveCamera position={cam} lookAt={[0, 2.2, 0]} fov={35} />
        <Grade />
        <Ground name="ground" />
        <Model name="temple" src={staticFile("ruins/007.glb")} position={[9, 0, -14]} rotation={[0, -0.5, 0]} />
        <AmbientLight name="amb" intensity={0.02} color="#8ea2c6" />
        <DirectionalLight name="sky" position={[-20, 30, -30]} intensity={0.08} color="#7d8fb5" />
        {/* the pyre from footage, whole */}
        <Footage t={t} name="pyre" clip="pyre" at={[-2.4, 0, 0]} height={5.2} face={cam} light={0.06} />
        {/* footage flames on a stack built in 3D */}
        {Array.from({ length: 5 }, (_, c) =>
          Array.from({ length: 5 }, (_, k) => (
            <Mesh key={`${c}-${k}`} name={`plank-${c}-${k}`} position={c % 2 ? [3, 0.1 + c * 0.19, (k - 2) * 0.36] : [3 + (k - 2) * 0.36, 0.1 + c * 0.19, 0]} rotation={[0, c % 2 ? 0 : Math.PI / 2, 0]}>
              <Box args={[2.0, 0.16, 0.26]} />
              <MeshStandardMaterial color="#2b1d12" roughness={0.9} emissive="#ff4a10" emissiveIntensity={c === 4 ? 0.12 : 0.05} />
            </Mesh>
          )),
        )}
        <Footage t={t} name="stack" clip="flames" at={[3, 0.45, 0]} height={3.4} face={cam} light={0.04} phase={0.9} />
        <Haze t={t} name="smoke" at={[0, 4.5, -3]} face={cam} width={22} depth={4} drift={0.25} tint="warm" />
      </Canvas3D>
    </AbsoluteFill>
  );
}

function Air() {
  const t = useCurrentFrame() / FPS;
  const cam: V = [0.6 + t * 0.02, 1.45, 5.2];
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Bed prompt="the hush of a large empty stone hall, faint air, distant birds outside, no music" />
      <Canvas3D>
        <PerspectiveCamera position={cam} lookAt={[0, 1.4, 0]} fov={36} />
        <Grade />
        <Ground name="floor" color="#2a241e" />
        {/* a bust on a plinth in a dark hall, columns behind */}
        <Mesh name="plinth" position={[0, 0.55, 0]}>
          <Box args={[0.6, 1.1, 0.6]} />
          <MeshStandardMaterial color="#b9b0a2" roughness={0.55} />
        </Mesh>
        <Model name="bust" src={staticFile("statues/marble_bust_01.glb")} position={[0, 1.1, 0]} rotation={[0, 0.5, 0]} scale={0.8} />
        {[-2.6, 2.6].map((x, i) => (
          <Mesh key={i} name={`col-${i}`} position={[x, 3, -3]}>
            <Box args={[0.8, 6, 0.8]} />
            <MeshStandardMaterial color="#6b5f50" roughness={0.9} />
          </Mesh>
        ))}
        <AmbientLight name="amb" intensity={0.012} color="#8ea2c6" />
        {/* the light the beams are: high on the left, down onto the bust */}
        <SpotLight name="shaft" position={[-2.6, 6, 0.2]} target={[0, 1.2, 0]} angle={0.32} penumbra={0.7} intensity={1.2} color="#ffd9a8" castShadow />
        <DustShafts t={t} name="hall" at={[0.75, 0, 0.1]} face={cam} height={6} spread={4} motes={140} beams={[{ x: 0, z: -0.45, lean: 0.5, width: 1.0 }, { x: 1.1, z: -0.8, lean: 0.5, width: 0.6 }]} />
        <Haze t={t} name="haze" at={[0, 1.6, -4]} face={cam} width={16} depth={2} drift={0.08} tint="cool" />
      </Canvas3D>
    </AbsoluteFill>
  );
}

function Sand() {
  const t = useCurrentFrame() / FPS;
  return (
    <AbsoluteFill style={{ background: "#000000" }}>
      <Bed prompt="dry wind over sand, a soft hiss of grains moving, no music" />
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 8]} lookAt={[0, 1.4, 0]} fov={35} />
        <Grade />
        <Ground name="ground" />
        <AmbientLight name="amb" intensity={0.05} />
        <Footage t={t} name="pyre" at={[0, 0, 0]} height={5} face={[0, 1.6, 8]} />
      </Canvas3D>
      <SandDrawing id={DRAWING} from={30} />
    </AbsoluteFill>
  );
}
const DRAWING = "rubicon";

registerRoot(() => (
  <>
    <Composition id="fire" component={Fire} width={1920} height={1080} fps={FPS} durationInFrames={150} />
    <Composition id="air" component={Air} width={1920} height={1080} fps={FPS} durationInFrames={150} />
    <Composition id="sand" component={Sand} width={1920} height={1080} fps={FPS} durationInFrames={330} />
  </>
));
