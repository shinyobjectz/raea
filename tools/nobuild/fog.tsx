import { Composition, registerRoot } from "raea";
import { FogVolume, VolumetricFog, type VolumetricFogProps } from "raea/pbr";
import { Camera3d } from "raea/camera";
import { LinearRgba, Srgba } from "raea/color";
import { DirectionalLight, SpotLight } from "raea/light";
import { Cuboid, Mesh3d } from "raea/mesh";
import { StandardMaterial } from "raea/pbr";
import { Transform } from "raea/transform";

// docs/nobuild.feature NB1a: each <VolumetricFog> tuning prop against the RAEA_FOG_* variable it replaced
// (tools/nobuild.sh props, its rows in tools/nobuild/props.tsv). A stone wall with a window, a spot light's shaft
// through it, a sun from the side, a glowing lamp (Solari's fog light), one volume of dusty air. Each composition but
// base sets one prop; base takes the variable.
const VARIANTS: Record<string, VolumetricFogProps> = {
  base: {},
  shadow: { shadow: "off" },
  exact: { heroExact: true },
  tricubic: { tricubic: true },
  epi: { epipolar: { lineGap: 16, spacing: 6, every: 4 } },
  grid: { tile: 8, slices: 96 },
  nohero: { hero: false },
  march: { mode: "march" },
  solari: { solari: { candidates: 1, frames: 1, skyRays: 2, sky: false, filter: false, guide: 1 } },
  solarioff: { solari: false },
};

function Room({ fog }: { fog: VolumetricFogProps }) {
  return (
    <>
      <Camera3d transform={Transform.fromXyz(4.4, 1.6, 4.2).lookingAt([-0.9, 2.4, -1.6])} />
      <VolumetricFog {...fog} />
      <Mesh3d transform={Transform.fromXyz(0, -0.1, 0)}><Cuboid half_size={[5, 0.1, 5]} /><StandardMaterial base_color="#6a645c" perceptual_roughness={0.9} /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(-3.2, 3.5, -3)}><Cuboid half_size={[1.8, 3.5, 0.25]} /><StandardMaterial base_color="#5a5248" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(2.6, 3.5, -3)}><Cuboid half_size={[2.4, 3.5, 0.25]} /><StandardMaterial base_color="#5a5248" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(-0.7, 1.3, -3)}><Cuboid half_size={[0.7, 1.3, 0.25]} /><StandardMaterial base_color="#5a5248" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(-0.7, 5.8, -3)}><Cuboid half_size={[0.7, 1.2, 0.25]} /><StandardMaterial base_color="#5a5248" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(0, 0.6, 0)}><Cuboid half_size={[0.4, 0.6, 0.4]} /><StandardMaterial base_color="#c8c0b4" /></Mesh3d>
      <Mesh3d transform={Transform.fromXyz(1.6, 1.8, -1)}><Cuboid half_size={[0.4, 0.6, 0.4]} /><StandardMaterial base_color="#000000" emissive={LinearRgba.from(Srgba.hex("#ff8a3c")).mul(20000)} /></Mesh3d>
      <SpotLight color="#ffe6c4" volumetric={true} shadows_enabled transform={Transform.fromXyz(-2.8, 7.4, -6).lookingAt([0, 1, 0])} intensity={2340000} outer_angle={0.2} inner_angle={0.17} />
      <DirectionalLight volumetric={true} shadow_maps_enabled illuminance={3000} transform={Transform.fromXyz(5, 3, 1).lookingAt([0, 0, 0])} />
      <FogVolume name="air" position={[0, 3.5, 1]} scale={[9.5, 7, 7.8]} density={0.1} color="#fff2e0" scattering={0.45} absorption={0.15} anisotropy={0.55} />
    </>
  );
}

registerRoot(() => (
  <>
    {Object.entries(VARIANTS).map(([id, fog]) => (
      <Composition key={id} id={id} component={() => <Room fog={fog} />} width={480} height={270} fps={30} durationInFrames={24} />
    ))}
  </>
));
