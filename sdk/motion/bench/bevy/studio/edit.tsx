// Studio fixture: a banana on a white turntable in a white cyc, as a product shot. One large soft key (a sun with
// soft shadows, PCSS), image-based fill from an HDRI, ambient occlusion at the contacts, high-key exposure (no tone
// curve, exposure -2.55: the cyc reads #f7f7f5 near the camera and white towards the horizon while the yellow stays
// saturated). The Environment's intensity is 0 to frame 9 and 1 from frame 20. studio/check.py takes features out
// by the markers fx:NAME and measures each; render equals its baked play (check.sh).
// media/studio_small_09_1k.hdr: Poly Haven, "Studio Small 09" by Sergej Majboroda,
// https://polyhaven.com/a/studio_small_09 (dl.polyhaven.org/file/ph-assets/HDRIs/hdr/1k/studio_small_09_1k.hdr),
// CC0 1.0, downloaded 2026-10-19.
import { AbsoluteFill, Composition, interpolate, registerRoot, staticFile, useCurrentFrame } from "@raea/motion";
import { AmbientOcclusion, Canvas3D, Cylinder, DirectionalLight, Environment, Mesh, MeshPhysicalMaterial, MeshStandardMaterial, PerspectiveCamera, Box, Group, Shadows, ToneMapping, manifold } from "@raea/motion/3d";

const SOFT = true; // fx:soft
const BANANA = manifold.sphere(1).smooth({ refine: 4 }).scale(1.1, 0.27, 0.27).refineToLength(0.05).warp((x, y, z) => {
  const t = x / 1.1;
  const pinch = 1 - 0.35 * t * t * t * t;
  return [x, y * pinch + 0.45 * t * t, z * pinch];
}).union(manifold.cylinder(0.05, 0.25).rotate(0, 90, 0).rotate(0, 0, 35).move(1.08, 0.5, 0));
// speckles for the peel: an SVG drawn once
const PEEL = "data:image/svg+xml;utf8," + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512"><rect width="512" height="512" fill="#f4cf3a"/>${Array.from({ length: 70 }, (_, i) => `<circle cx="${(i * 73) % 512}" cy="${(i * 151) % 512}" r="${2 + (i % 5)}" fill="#6b4a1e" opacity="0.55"/>`).join("")}<rect y="0" width="512" height="40" fill="#b8c24a" opacity="0.5"/></svg>`);

function Studio() {
  const f = useCurrentFrame();
  return (
    <AbsoluteFill style={{ background: "#ffffff" }}>
      <Canvas3D>
        <PerspectiveCamera position={[0, 1.6, 5.2]} lookAt={[0, 0.45, 0]} fov={32} />
        <ToneMapping mode="none" exposure={-2.55} />
        <Shadows bias={0.2} distance={30} mapSize={4096} />
        <DirectionalLight position={[-3, 6, 3]} intensity={2.2} softShadow={SOFT ? 10 : false} />
        <Environment src={staticFile("studio_small_09_1k.hdr")} intensity={interpolate(f, [9, 20], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })} rotation={0.6} />{/* fx:env */}
        <AmbientOcclusion quality="high" thickness={0.15} />{/* fx:ao */}
        <Mesh name="floor" position={[0, -0.05, 0]}><Box args={[200, 0.1, 200]} /><MeshPhysicalMaterial color="#f7f7f5" roughness={1} reflectance={0} /></Mesh>
        <Group rotation={[0, f * 0.025, 0]}>
          <Mesh name="susan" position={[0, 0.06, 0]}><Cylinder args={[1.25, 1.25, 0.12]} /><MeshStandardMaterial color="#f4f4f2" roughness={0.35} /></Mesh>
          <Mesh name="banana" shape={BANANA} position={[0, 0.38, 0]} rotation={[0.15, 0.6, 0.1]} scale={0.95}>
            <MeshStandardMaterial color="#ffffff" map={PEEL} roughness={0.55} />
          </Mesh>
        </Group>
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="studio" component={Studio} width={960} height={540} fps={30} durationInFrames={60} />);
