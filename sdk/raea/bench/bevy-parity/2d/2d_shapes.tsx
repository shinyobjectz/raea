// Bevy v0.20.0 examples/2d/2d_shapes.rs in raea's TSX (docs/control.feature FC2): a Camera2d and twelve Mesh2d
// primitives in a row at y 75 (Circle, CircularSector, CircularSegment, Ellipse, Annulus, Capsule2d, Rhombus,
// Rectangle, RegularPolygon, Triangle2d, Segment2d, Polyline2d), x spread from -500 to 500, each a ColorMaterial of
// Color::hsl(360 i / 12, 0.95, 0.7); below them at y -75 ten rings (to_ring(5) or a Ring of two shapes), spread as
// twelve would be, coloured by i / 12. In the bottom left the ROTATE and WIREFRAME feathers checkboxes, unchecked,
// under helpers/theme.rs basic_example_theme(WHITE), so nothing turns and no wireframe draws; Bevy's default 1280 x 720
// window and clear colour.
import type { ReactElement } from "react";
import { AbsoluteFill, Composition, registerRoot } from "raea";
import { Canvas3D } from "raea/3d";
import { Hsla, Srgba } from "raea/color";
import { UiTheme } from "raea/feathers";
import { Annulus, Capsule2d, Circle, CircularSector, CircularSegment, ColorMaterial, Ellipse, Mesh2d, Polyline2d, Rectangle, RegularPolygon, Rhombus, Segment2d, Triangle2d } from "raea/sprite";
import { Transform } from "raea/transform";
import { BottomLeft, OptionCheckbox, basic_example_theme } from "../helpers/theme";

const X_EXTENT = 1000, Y_EXTENT = 150, THICKNESS = 5;
const TRIANGLE: [number, number][] = [[0, 50], [-50, -50], [50, -50]];

const SHAPES = [
  <Circle radius={50} />,
  <CircularSector arc={{ radius: 50, half_angle: 1 }} />,
  <CircularSegment arc={{ radius: 50, half_angle: 1.25 }} />,
  <Ellipse half_size={[25, 50]} />,
  <Annulus inner_circle={{ radius: 25 }} outer_circle={{ radius: 50 }} />,
  <Capsule2d radius={25} half_length={25} />,
  <Rhombus half_diagonals={[37.5, 50]} />,
  <Rectangle half_size={[25, 50]} />,
  <RegularPolygon circumcircle={{ radius: 50 }} sides={6} />,
  <Triangle2d vertices={TRIANGLE} />,
  <Segment2d vertices={[[-50, 50], [50, -50]]} />,
  <Polyline2d vertices={[[-50, 50], [0, -50], [50, 50]]} />,
];

const RINGS = [
  <Circle radius={50} ring={THICKNESS} />,
  <CircularSector arc={{ radius: 50, half_angle: 1 }} inner={{ arc: { radius: 45, half_angle: 1 } }} />,
  <CircularSegment arc={{ radius: 50, half_angle: 1.25 }} ring={THICKNESS} />,
  // concentric ellipses, each half size less THICKNESS (Ellipse has no Inset)
  <Ellipse half_size={[25, 50]} inner={{ half_size: [25 - THICKNESS, 50 - THICKNESS] }} />,
  <Circle radius={50} inner={{ radius: 25 }} />,
  <Capsule2d radius={25} half_length={25} ring={THICKNESS} />,
  <Rhombus half_diagonals={[37.5, 50]} ring={THICKNESS} />,
  <Rectangle half_size={[25, 50]} ring={THICKNESS} />,
  <RegularPolygon circumcircle={{ radius: 50 }} sides={6} ring={THICKNESS} />,
  <Triangle2d vertices={TRIANGLE} ring={THICKNESS} />,
];

/** a row of shapes spread over X_EXTENT at height y, `n` places wide, each coloured by its place */
function row(shapes: ReactElement[], n: number, y: number) {
  return shapes.map((shape, i) => (
    <Mesh2d key={`${y} ${i}`} transform={Transform.fromXyz(-X_EXTENT / 2 + (i / (n - 1)) * X_EXTENT, y, 0)}>
      {shape}
      <ColorMaterial color={new Hsla((360 * i) / n, 0.95, 0.7).to_srgba()} />
    </Mesh2d>
  ));
}

function Shot() {
  return (
    <AbsoluteFill>
      <Canvas3D layered clearColor="rgb(43, 44, 47)">
        {row(SHAPES, SHAPES.length, Y_EXTENT / 2)}
        {row(RINGS, RINGS.length + 2, -Y_EXTENT / 2)}
      </Canvas3D>
      <UiTheme theme={basic_example_theme(new Srgba(1, 1, 1, 1))}>
        <BottomLeft>
          <OptionCheckbox name="ROTATE" />
          <OptionCheckbox name="WIREFRAME" />
        </BottomLeft>
      </UiTheme>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="main" component={Shot} durationInFrames={11} fps={60} width={1280} height={720} />);
