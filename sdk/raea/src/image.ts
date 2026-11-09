// docs/sdk.feature S1b (M1): a Bevy-named entry point, re-exporting what today lives in the files named. Sources move
// into it at M5; until then the old subpaths and this one export the same values.
// raea/image (bevy_image): pictures, on the page and in the world.
export { Img, type ImgProps } from "@raea/video/media";
export { ImagePlane, Plate, type ImagePlaneProps, type PlateProps } from "./3d";
export { TextureAtlasBuilder, type BuiltAtlas, type AtlasSprite } from "@raea/render/3d/atlas";
