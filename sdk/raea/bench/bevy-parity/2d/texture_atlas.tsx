// Bevy v0.20.0 examples/2d/texture_atlas.rs in raea's TSX (docs/control.feature FC2): every picture of
// textures/rpg, as asset_server.load_folder gives them (depth first, each folder in its read_dir order, listed below
// as this checkout's file system orders them), packed four times by Bevy's TextureAtlasBuilder: with no padding and
// with 6 x 6 px, each sampled linear and nearest. The linear atlases drawn whole at (-250, -160) and (250, -160),
// half size; the vendor picked from each atlas at (x, 80), three times its size, x -350, -150, 150 and 350; Text2d
// labels in FiraSans-Bold, 42 px at y 250 and 25 px 110 above each vendor, centred. Camera2d, Bevy's default clear
// colour and 1280 x 720 window (ImagePlugin::default_nearest, which every picture drawn here overrides).
import { AbsoluteFill, Composition, registerRoot, staticFile } from "raea";
import { Canvas3D, Sprite } from "raea/3d";
import { TextureAtlasBuilder } from "raea/sprite";
import { Transform } from "raea/transform";
import { Text2d } from "raea/ui";

const RPG = [
  "textures/rpg/ui/generic-rpg-ui-inventario.png",
  "textures/rpg/ui/generic-rpg-ui-text-box.png",
  "textures/rpg/ui/generic-rpg-ui-inventario01.png",
  "textures/rpg/ui/generic-rpg-ui-inventario03.png",
  "textures/rpg/ui/generic-rpg-ui-inventario02.png",
  "textures/rpg/ui/generic-rpg-ui-inventario04.png",
  "textures/rpg/mobs/worm-run-idle.png",
  "textures/rpg/mobs/slime-blue.png",
  "textures/rpg/mobs/boss_bee.png",
  "textures/rpg/mobs/fox-run.png",
  "textures/rpg/mobs/slime-green.png",
  "textures/rpg/mobs/slime-orange.png",
  "textures/rpg/mobs/kobold-idle.png",
  "textures/rpg/props/generic-rpg-board02.png",
  "textures/rpg/props/generic-rpg-barrel02.png",
  "textures/rpg/props/generic-rpg-grass02.png",
  "textures/rpg/props/generic-rpg-barrel03.png",
  "textures/rpg/props/generic-rpg-board03.png",
  "textures/rpg/props/generic-rpg-treasure-closed.png",
  "textures/rpg/props/generic-rpg-board01.png",
  "textures/rpg/props/generic-rpg-barrel01.png",
  "textures/rpg/props/generic-rpg-grass01.png",
  "textures/rpg/props/generic-rpg-board04.png",
  "textures/rpg/props/generic-rpg-flowers.png",
  "textures/rpg/props/generic-rpg-fence-raw03.png",
  "textures/rpg/props/generic-rpg-fence-raw17.png",
  "textures/rpg/props/generic-rpg-rock01.png",
  "textures/rpg/props/generic-rpg-loot01.png",
  "textures/rpg/props/generic-rpg-fence-raw16.png",
  "textures/rpg/props/generic-rpg-fence-raw02.png",
  "textures/rpg/props/generic-rpg-fence-raw14.png",
  "textures/rpg/props/generic-rpg-loot03.png",
  "textures/rpg/props/generic-rpg-rock03.png",
  "textures/rpg/props/generic-rpg-rock02.png",
  "textures/rpg/props/generic-rpg-loot02.png",
  "textures/rpg/props/generic-rpg-fence-raw01.png",
  "textures/rpg/props/generic-rpg-fence-raw15.png",
  "textures/rpg/props/generic-rpg-fence-raw11.png",
  "textures/rpg/props/generic-rpg-fence-raw05.png",
  "textures/rpg/props/generic-rpg-rock06.png",
  "textures/rpg/props/generic-rpg-rod.png",
  "textures/rpg/props/generic-rpg-fence-raw04.png",
  "textures/rpg/props/generic-rpg-fence-raw10.png",
  "textures/rpg/props/generic-rpg-bridge.png",
  "textures/rpg/props/generic-rpg-fence-raw06.png",
  "textures/rpg/props/generic-rpg-fence-raw12.png",
  "textures/rpg/props/generic-rpg-loot05.png",
  "textures/rpg/props/generic-rpg-rock05.png",
  "textures/rpg/props/generic-rpg-fence08.png",
  "textures/rpg/props/generic-rpg-fence09.png",
  "textures/rpg/props/generic-rpg-rock04.png",
  "textures/rpg/props/generic-rpg-loot04.png",
  "textures/rpg/props/generic-rpg-fence-raw13.png",
  "textures/rpg/props/generic-rpg-fence-raw07.png",
  "textures/rpg/props/generic-rpg-fence-raw22.png",
  "textures/rpg/props/generic-rpg-fence04.png",
  "textures/rpg/props/generic-rpg-fence10.png",
  "textures/rpg/props/generic-rpg-fence11.png",
  "textures/rpg/props/generic-rpg-fence05.png",
  "textures/rpg/props/generic-rpg-flower01.png",
  "textures/rpg/props/generic-rpg-fence-raw23.png",
  "textures/rpg/props/generic-rpg-fence-raw21.png",
  "textures/rpg/props/generic-rpg-fence-raw09.png",
  "textures/rpg/props/generic-rpg-flower03.png",
  "textures/rpg/props/generic-rpg-fence-complete.png",
  "textures/rpg/props/generic-rpg-fence13.png",
  "textures/rpg/props/generic-rpg-fence07.png",
  "textures/rpg/props/generic-rpg-fence06.png",
  "textures/rpg/props/generic-rpg-fence12.png",
  "textures/rpg/props/generic-rpg-flower02.png",
  "textures/rpg/props/generic-rpg-fence-raw08.png",
  "textures/rpg/props/generic-rpg-fence-raw20.png",
  "textures/rpg/props/generic-rpg-tree02.png",
  "textures/rpg/props/generic-rpg-fence-raw18.png",
  "textures/rpg/props/generic-rpg-fence-raw24.png",
  "textures/rpg/props/generic-rpg-fence02.png",
  "textures/rpg/props/generic-rpg-fence03.png",
  "textures/rpg/props/generic-rpg-fence-raw25.png",
  "textures/rpg/props/generic-rpg-fence-raw19.png",
  "textures/rpg/props/generic-rpg-tree01.png",
  "textures/rpg/props/generic-rpg-house-inn.png",
  "textures/rpg/props/generic-rpg-fence01.png",
  "textures/rpg/props/generic-rpg-fence14.png",
  "textures/rpg/props/generic-rpg-crate02.png",
  "textures/rpg/props/generic-rpg-crate03.png",
  "textures/rpg/props/generic-rpg-crate01.png",
  "textures/rpg/props/generic-rpg-fish04.png",
  "textures/rpg/props/generic-rpg-mini-lake.png",
  "textures/rpg/props/generic-rpg-fish01.png",
  "textures/rpg/props/generic-rpg-fish02.png",
  "textures/rpg/props/generic-rpg-fish03.png",
  "textures/rpg/props/generic-rpg-trasure-open.png",
  "textures/rpg/tiles/generic-rpg-tile35.png",
  "textures/rpg/tiles/generic-rpg-tile21.png",
  "textures/rpg/tiles/generic-rpg-tile09.png",
  "textures/rpg/tiles/generic-rpg-tile08.png",
  "textures/rpg/tiles/generic-rpg-tile20.png",
  "textures/rpg/tiles/generic-rpg-tile34.png",
  "textures/rpg/tiles/generic-rpg-tile22.png",
  "textures/rpg/tiles/generic-rpg-tile37.png",
  "textures/rpg/tiles/generic-rpg-tile23.png",
  "textures/rpg/tiles/generic-rpg-tile27.png",
  "textures/rpg/tiles/generic-rpg-tile33.png",
  "textures/rpg/tiles/generic-rpg-tile32.png",
  "textures/rpg/tiles/generic-rpg-tile26.png",
  "textures/rpg/tiles/generic-rpg-tile18.png",
  "textures/rpg/tiles/generic-rpg-tile30.png",
  "textures/rpg/tiles/generic-rpg-tile24.png",
  "textures/rpg/tiles/generic-rpg-tile25.png",
  "textures/rpg/tiles/generic-rpg-tile31.png",
  "textures/rpg/tiles/generic-rpg-tile19.png",
  "textures/rpg/tiles/generic-rpg-tile56.png",
  "textures/rpg/tiles/generic-rpg-tile42.png",
  "textures/rpg/tiles/generic-rpg-tile43.png",
  "textures/rpg/tiles/generic-rpg-tile57.png",
  "textures/rpg/tiles/generic-rpg-tile41.png",
  "textures/rpg/tiles/generic-rpg-tile55.png",
  "textures/rpg/tiles/generic-rpg-tile69.png",
  "textures/rpg/tiles/generic-rpg-tile68.png",
  "textures/rpg/tiles/generic-rpg-tile54.png",
  "textures/rpg/tiles/generic-rpg-tile40.png",
  "textures/rpg/tiles/generic-rpg-tile44.png",
  "textures/rpg/tiles/generic-rpg-tile50.png",
  "textures/rpg/tiles/generic-rpg-tile51.png",
  "textures/rpg/tiles/generic-rpg-tile45.png",
  "textures/rpg/tiles/generic-rpg-tile53.png",
  "textures/rpg/tiles/generic-rpg-tile47.png",
  "textures/rpg/tiles/generic-rpg-tile46.png",
  "textures/rpg/tiles/generic-rpg-tile52.png",
  "textures/rpg/tiles/generic-rpg-tile63.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall01.png",
  "textures/rpg/tiles/generic-rpg-tile62.png",
  "textures/rpg/tiles/generic-rpg-tile60.png",
  "textures/rpg/tiles/generic-rpg-tile48.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall02.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall03.png",
  "textures/rpg/tiles/generic-rpg-tile49.png",
  "textures/rpg/tiles/generic-rpg-tile61.png",
  "textures/rpg/tiles/generic-rpg-tile59.png",
  "textures/rpg/tiles/generic-rpg-tile65.png",
  "textures/rpg/tiles/generic-rpg-tile71.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall07.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall06.png",
  "textures/rpg/tiles/generic-rpg-tile70.png",
  "textures/rpg/tiles/generic-rpg-tile64.png",
  "textures/rpg/tiles/generic-rpg-tile58.png",
  "textures/rpg/tiles/generic-rpg-tile66.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall04.png",
  "textures/rpg/tiles/generic-rpg-tile-waterfall05.png",
  "textures/rpg/tiles/generic-rpg-tile67.png",
  "textures/rpg/tiles/generic-rpg-tile14.png",
  "textures/rpg/tiles/generic-rpg-tile28.png",
  "textures/rpg/tiles/generic-rpg-tile29.png",
  "textures/rpg/tiles/generic-rpg-tile01.png",
  "textures/rpg/tiles/generic-rpg-tile15.png",
  "textures/rpg/tiles/generic-rpg-tile03.png",
  "textures/rpg/tiles/generic-rpg-tile17.png",
  "textures/rpg/tiles/generic-rpg-tile16.png",
  "textures/rpg/tiles/generic-rpg-tile02.png",
  "textures/rpg/tiles/generic-rpg-tile06.png",
  "textures/rpg/tiles/generic-rpg-tile12.png",
  "textures/rpg/tiles/generic-rpg-tile13.png",
  "textures/rpg/tiles/generic-rpg-tile07.png",
  "textures/rpg/tiles/generic-rpg-tile39.png",
  "textures/rpg/tiles/generic-rpg-tile11.png",
  "textures/rpg/tiles/generic-rpg-tile05.png",
  "textures/rpg/tiles/generic-rpg-tile04.png",
  "textures/rpg/tiles/generic-rpg-tile10.png",
  "textures/rpg/tiles/generic-rpg-tile38.png",
  "textures/rpg/tiles/generic-rpg-Slice.png",
  "textures/rpg/chars/sensei/sensei.png",
  "textures/rpg/chars/mani/mani-idle-run.png",
  "textures/rpg/chars/hat-guy/hat-guy.png",
  "textures/rpg/chars/vendor/generic-rpg-vendor.png",
  "textures/rpg/chars/gabe/gabe-idle-run.png",
].map((p) => staticFile(p));
const VENDOR = staticFile("textures/rpg/chars/vendor/generic-rpg-vendor.png");
const FONT = staticFile("fonts/FiraSans-Bold.ttf");

function atlas(padding: [number, number], filter: "linear" | "nearest") {
  const b = new TextureAtlasBuilder().padding(padding);
  RPG.forEach((p) => b.addTexture(p));
  return b.build({ sampler: { filter } });
}
const linear = atlas([0, 0], "linear");
const nearest = atlas([0, 0], "nearest");
const linearPadded = atlas([6, 6], "linear");
const nearestPadded = atlas([6, 6], "nearest");
const CONFIGS = [["Linear", linear, -350], ["Nearest", nearest, -150], ["Linear", linearPadded, 150], ["Nearest", nearestPadded, 350]] as const;
const BASE_Y = 80;

function Shot() {
  return (
    <AbsoluteFill>
      <Canvas3D layered clearColor="rgb(43, 44, 47)">
        <Sprite {...linear.image} x={640 - 250} y={360 + 160} scale={0.5} />
        <Sprite {...linearPadded.image} x={640 + 250} y={360 + 160} scale={0.5} />
        {CONFIGS.map(([, a, x]) => <Sprite key={x} {...a.sources.handle(VENDOR)} x={640 + x} y={360 - BASE_Y} scale={3} />)}
        <Text2d transform={Transform.fromXyz(-250, 250, 0)} justify="Center" font={FONT} font_size={42}>No padding</Text2d>
        <Text2d transform={Transform.fromXyz(250, 250, 0)} justify="Center" font={FONT} font_size={42}>Padding</Text2d>
        {CONFIGS.map(([label, , x]) => (
          <Text2d key={x} transform={Transform.fromXyz(x, BASE_Y + 110, 0)} justify="Center" font={FONT} font_size={25}>{label}</Text2d>
        ))}
      </Canvas3D>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="main" component={Shot} durationInFrames={31} fps={60} width={1280} height={720} />);
