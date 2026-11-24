import { Composition, registerRoot, useCurrentFrame } from "raea";
import { Camera3d } from "raea/camera";
import { LinearRgba, Srgba } from "raea/color";
import { DirectionalLight, Pathtracer, type PathtracerProps, SolariLighting, type SolariLightingProps } from "raea/light";
import { Cuboid, Mesh3d } from "raea/mesh";
import { FogVolume, StandardMaterial } from "raea/pbr";
import { Transform } from "raea/transform";

// docs/nobuild.feature NB1a: <Pathtracer samples media> against RAEA_LIGHT_SPP and RAEA_PT_MEDIA, and <SolariLighting
// dynamic> against RAEA_SOLARI_DYNAMIC (tools/nobuild.sh props). A floor, a glowing panel, a volume of air, and a slab
// over the floor that is there on the first frame only: hidden after, it still shadows the floor unless the traced scene
// follows what moves (dynamic).
const TRACED: Record<string, { pt?: PathtracerProps; solari?: SolariLightingProps }> = {
  trace: { pt: {} },
  spp: { pt: { samples: 4 } },
  nomedia: { pt: { media: false } },
  solari: { solari: {} },
  dynamic: { solari: { dynamic: true } },
};

function Scene({ pt, solari }: { pt?: PathtracerProps; solari?: SolariLightingProps }) {
  const f = useCurrentFrame();
  return (
    <>
      <Camera3d transform={Transform.fromXyz(0, 2.4, 5.5).lookingAt([0, 0.4, 0])}>
        {pt && <Pathtracer {...pt} />}
        {solari && <SolariLighting {...solari} />}
      </Camera3d>
      <DirectionalLight illuminance={8000} transform={Transform.fromXyz(1, 6, 1).lookingAt([0, 0, 0])} />
      <Mesh3d><Cuboid half_size={[4, 0.05, 4]} /><StandardMaterial base_color="#b0aaa0" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(-1.8, 1.2, -1.5)}><Cuboid half_size={[0.6, 0.6, 0.05]} /><StandardMaterial base_color="#000000" emissive={LinearRgba.from(Srgba.hex("#8ac4ff")).mul(4000)} /></Mesh3d>
      {f < 1 && <Mesh3d transform={Transform.fromXyz(0.4, 1.6, 0.4)}><Cuboid half_size={[1.2, 0.05, 1.2]} /><StandardMaterial base_color="#202020" /></Mesh3d>}
      <FogVolume name="air" position={[0, 1.5, 0]} scale={[8, 3, 8]} density={0.08} />
    </>
  );
}

registerRoot(() => (
  <>
    {Object.entries(TRACED).map(([id, t]) => (
      <Composition key={id} id={id} component={() => <Scene {...t} />} width={240} height={135} fps={30} durationInFrames={id === "solari" || id === "dynamic" ? 12 : 3} />
    ))}
  </>
));
