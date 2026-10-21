// void's cat as the logo sees it: the Logo pose, from the angle fit.py found (build.py's cam_* numbers, its
// orthographic camera stood in for by a narrow lens far away, and moved half a cell so the cells fall on the logo's pixels: its viewBox starts at
// x -1.5), flat. Frame 0 solid, 1 pixel at one cell a logo pixel
// (256 px over 32 cells), 2 the site's grain.
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientLight, Canvas3D, DirectionalLight, Model, PerspectiveCamera, ToneMapping } from "@raea/motion/3d";

const LOOKS = ["solid", "pixel", "site"] as const;

function Logo() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#ffffff" }}>
      <Canvas3D>
        <PerspectiveCamera position={[-24.6536, 13.6426, 53.0863]} lookAt={[-0.0149, 0.4265, -0.0011]} fov={0.79203} near={40} far={80} />
        <ToneMapping mode="none" />
        <AmbientLight intensity={0.5} />
        <DirectionalLight position={[2.5, 5, 4]} intensity={1.4} />
        <Model src={staticFile("cat.glb")} animation="Logo" flat look={LOOKS[f % 3]} pixelSize={8} edgeColor="none" theme="light" />
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="cat-logo" component={Logo} width={256} height={256} fps={30} durationInFrames={3} />);
