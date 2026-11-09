// Bevy v0.20.0 examples/2d/text2d.rs in raea's TSX (docs/control.feature FC2): Bevy's own Text2d inside the Canvas3D.
// " translation ", " rotation " and " scale " in FiraSans-Bold at 50 px, centred, on a half-black background with
// Text2dShadow's default, moving on a circle of 100 px about (-400, 0), turning by cos(t) and scaled by
// (sin(t) + 1.1) * 2 at (400, 0), t the elapsed seconds (a 1/60 s Duration a frame, as f32); two 300 x 200 boxes
// (srgb 0.25, 0.25, 0.55) at (0, -250) and (320, -250), each with 35 px text wrapped in its TextBounds, Unicode
// (WordBoundary, underlined) and AnyCharacter linebreaks, one above the box (z 1) with a shadow of the box's colour
// darker(0.05); FontSmoothing::None text at (-400, -250), centred; a 10 x 10 LIGHT_CYAN sprite at (0, 250) over four
// " Anchor" texts at z -1, one at each corner Anchor, on WHITE.darker(0.8), with a "::" span (LIGHT_GREY on DARK_BLUE)
// and the anchor's Debug in its colour on that colour darker(0.3). Camera2d, Bevy's default clear colour and
// 1280 x 720 window.
import { AbsoluteFill, Composition, registerRoot, staticFile, useCurrentFrame } from "raea";
import { Canvas3D } from "raea/3d";
import { Shape2D } from "raea/sprite";
import { Srgba } from "raea/color";
import { Quat, Transform } from "raea/transform";
import { Text2d, TextSpan } from "raea/ui";

const f = Math.fround;
const FONT = staticFile("fonts/FiraSans-Bold.ttf");
const HALF_BLACK = new Srgba(0, 0, 0, 0.5);
const BOX = new Srgba(0.25, 0.25, 0.55, 1);
const BOX_RGB: [number, number, number] = [0.25, 0.25, 0.55];
const BOX_SHADOW = BOX.darker(0.05);
const WHITE_DARKER = new Srgba(1, 1, 1, 1).darker(0.8);
const LIGHT_GREY = new Srgba(0.827, 0.827, 0.827, 1);
const DARK_BLUE = new Srgba(0, 0, 0.545, 1);
// Bevy's Anchor constants with their Debug, and the colour each is shown in
const ANCHORS: [[number, number], string, Srgba][] = [
  [[-0.5, 0.5], "Anchor(Vec2(-0.5, 0.5))", new Srgba(1, 0.627, 0.478, 1)],
  [[0.5, 0.5], "Anchor(Vec2(0.5, 0.5))", new Srgba(0.565, 0.933, 0.565, 1)],
  [[0.5, -0.5], "Anchor(Vec2(0.5, -0.5))", new Srgba(0.678, 0.847, 0.902, 1)],
  [[-0.5, -0.5], "Anchor(Vec2(-0.5, -0.5))", new Srgba(1, 1, 0.878, 1)],
];

function Shot() {
  // Time::elapsed_secs: f frames of Bevy's 1/60 s Duration, as f32
  const t = f((useCurrentFrame() * 16_666_667) / 1e9);
  const s = f(f(f(Math.sin(t)) + 1.1) * 2);
  const big = { font: FONT, font_size: 50, justify: "Center" as const, text_background_color: HALF_BLACK, shadow: true as const };
  const small = { font: FONT, font_size: 35 };
  return (
    <AbsoluteFill>
      <Canvas3D layered clearColor="rgb(43, 44, 47)">
        <Text2d {...big} transform={Transform.fromXyz(f(f(100 * f(Math.sin(t))) - 400), f(100 * f(Math.cos(t))), 0)}> translation </Text2d>
        <Text2d {...big} transform={{ rotation: Quat.fromRotationZ(f(Math.cos(t))) }}> rotation </Text2d>
        <Text2d {...big} transform={{ translation: [400, 0, 0], scale: [s, s, 1] }}> scale </Text2d>
        <Shape2D size={[300, 200]} srgb={BOX_RGB} x={640} y={360 + 250} />
        <Text2d {...small} justify="Left" linebreak="WordBoundary" bounds={[300, 200]} transform={Transform.fromXyz(0, -250, 1)} shadow={{ color: BOX_SHADOW }} underline>
          {"this text wraps in the box\n(Unicode linebreaks)"}
        </Text2d>
        <Shape2D size={[300, 200]} srgb={BOX_RGB} x={640 + 320} y={360 + 250} />
        <Text2d {...small} justify="Left" linebreak="AnyCharacter" bounds={[300, 200]} transform={Transform.fromXyz(320, -250, 1)} shadow={{ color: BOX_SHADOW }}>
          {"this text wraps in the box\n(AnyCharacter linebreaks)"}
        </Text2d>
        <Text2d {...small} font_smoothing="None" justify="Center" transform={Transform.fromXyz(-400, -250, 0)} shadow>
          {"This text has\nFontSmoothing::None\nAnd Justify::Center"}
        </Text2d>
        <Shape2D size={[10, 10]} srgb={[0.878, 1, 1]} x={640} y={360 - 250} />
        {ANCHORS.map(([anchor, name, color]) => (
          <Text2d key={name} {...small} anchor={anchor} text_background_color={WHITE_DARKER} transform={Transform.fromXyz(0, 250, -1)}>
            {" Anchor"}
            <TextSpan {...small} color={LIGHT_GREY} text_background_color={DARK_BLUE}>::</TextSpan>
            <TextSpan {...small} color={color} text_background_color={color.darker(0.3)}>{`${name} `}</TextSpan>
          </Text2d>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="main" component={Shot} durationInFrames={61} fps={60} width={1280} height={720} />);
