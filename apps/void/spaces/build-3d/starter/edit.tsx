// The Build 3D space's starter object: a rock garden the space's panels work on. The outliner picks a part, the
// materials row paints the picked part; both reach the scene as the space's keys (@raea/motion spaceIn), so the scene
// is a pure fold over its input like any game, and a recorded session replays to the same pixels.
import { AbsoluteFill, Composition, registerRoot, spaceIn, useCurrentFrame, useGameState, type FrameInput } from "@raea/motion";
import { AmbientLight, Box, Canvas3D, DirectionalLight, Mesh, MeshStandardMaterial, PerspectiveCamera, Sphere } from "@raea/motion/3d";

const CW = 1280, CH = 720;
const PARTS = ["Camera", "Sun", "Ground", "Rock 1", "Rock 2", "Rock 3"];
const START: Record<string, string> = { Sun: "#fff1d6", Ground: "#5f7a3c", "Rock 1": "#8a8178", "Rock 2": "#8a8178", "Rock 3": "#8a8178" };

type S = { colors: Record<string, string>; picked: string; material: string | null };
const INITIAL: S = { colors: START, picked: "Ground", material: null };

const step = (s: S, input: FrameInput): S => {
  const part = spaceIn<{ part: string }>(input, "outliner")?.part ?? s.picked;
  const m = spaceIn<{ material: string; color: string }>(input, "materials");
  let next = part === s.picked ? s : { ...s, picked: part };
  // a material picked (a change, not the one standing when the scene first heard the row) paints the part picked now
  if (m && s.material === null) return { ...next, material: m.material };
  if (m && m.material !== s.material) {
    next = { ...next, material: m.material, colors: next.picked in START ? { ...next.colors, [next.picked]: m.color } : next.colors };
  }
  return next;
};

const ROCKS: [string, [number, number, number], number][] = [["Rock 1", [-1.6, 0.45, 0.4], 0.6], ["Rock 2", [0.4, 0.7, -0.6], 0.85], ["Rock 3", [1.7, 0.35, 0.9], 0.45]];

function Garden() {
  const f = useCurrentFrame();
  const s = useGameState(INITIAL, step);
  const a = f / 600;
  // the picked part lit a little blue from inside, so its own colour still shows
  const glow = (name: string) => (s.picked === name ? "#16246e" : "#000000");
  return (
    <AbsoluteFill style={{ background: "#0d0f14" }}>
      <Canvas3D>
        <PerspectiveCamera position={[Math.sin(a) * 7, 4.2, Math.cos(a) * 7]} lookAt={[0, 0.4, 0]} fov={45} />
        <AmbientLight intensity={0.35} />
        <DirectionalLight position={[5, 9, 4]} intensity={1.2} color={s.colors.Sun} />
        {/* the sun's marker sits on the light's own ray, so it casts no shadow (it shadowed Rock 3) */}
        <Mesh name="sun" position={[5, 6, 4]} shadow={false}>
          <Sphere args={[0.35]} />
          <MeshStandardMaterial color={s.colors.Sun} emissive={s.colors.Sun} />
        </Mesh>
        <Mesh name="ground" position={[0, -0.05, 0]}>
          <Box args={[8, 0.1, 8]} />
          <MeshStandardMaterial color={s.colors.Ground} roughness={0.95} emissive={glow("Ground")} />
        </Mesh>
        {ROCKS.map(([name, p, r]) => (
          <Mesh key={name} name={name} position={p} scale={[1, 0.75, 1]}>
            <Sphere args={[r]} />
            <MeshStandardMaterial color={s.colors[name]} roughness={0.8} emissive={glow(name)} />
          </Mesh>
        ))}
      </Canvas3D>
      <div style={{ position: "absolute", left: 24, bottom: 20, fontFamily: "Inter", fontSize: 18, color: "#c9ccd6" }}>{`${s.picked}${s.material ? " · " + s.material : ""}`}</div>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="edit" component={Garden} width={CW} height={CH} fps={60} durationInFrames={1000000000} />);
