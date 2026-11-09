// bevy_image's TextureAtlasBuilder: pictures packed into one atlas, which raea-sys-sprite builds with Bevy's own
// builder (systems/sprite atlas.rs), each picture added in the order given (the packer's ties go by that order).
import { samplerSpec, type SamplerSpec } from "./materials";

/** What a <Sprite> takes to show a built atlas: the whole of it, or one of its pictures (`atlasPick`). */
export interface AtlasSprite {
  src: string;
  sampler?: SamplerSpec;
  atlasPick?: number;
}

/** A built atlas: `image` the whole atlas as a sprite's picture, `sources.handle(texture)` one picture of it
 * (TextureAtlasSources::handle). */
export interface BuiltAtlas {
  image: AtlasSprite;
  sources: { handle(texture: string): AtlasSprite };
}

/** Pictures packed into one atlas, as Bevy's TextureAtlasBuilder: `padding([x, y])` px around each, `addTexture` in
 * order, then `build({ sampler })` (the atlas's own sampler, Bevy's default otherwise).
 * @bevy bevy_image::texture_atlas_builder::TextureAtlasBuilder
 * @bevy bevy_image::texture_atlas::TextureAtlasSources */
export class TextureAtlasBuilder {
  private pad: [number, number] = [0, 0];
  private textures: string[] = [];

  padding(p: [number, number]): this {
    if (!(Array.isArray(p) && p.length === 2 && p.every((n) => Number.isInteger(n) && n >= 0))) throw new TypeError(`TextureAtlasBuilder: padding is [x, y], whole px of 0 or more, got ${JSON.stringify(p)}`);
    this.pad = p;
    return this;
  }

  addTexture(src: string): this {
    if (typeof src !== "string" || src === "" || src.startsWith("render:") || src.startsWith("atlas:")) throw new TypeError(`TextureAtlasBuilder: a texture is a picture (staticFile("x.png")), got ${JSON.stringify(src)}`);
    this.textures.push(src);
    return this;
  }

  build({ sampler }: { sampler?: SamplerSpec } = {}): BuiltAtlas {
    if (this.textures.length === 0) throw new TypeError("TextureAtlasBuilder: add a texture before build()");
    if (sampler !== undefined) samplerSpec("TextureAtlasBuilder: sampler", sampler);
    const textures = [...this.textures];
    // the sampler is in the key: one list packed twice with two samplers is two atlases, as in Bevy
    const src = `atlas:${JSON.stringify({ textures, padding: this.pad, sampler: sampler ?? null })}`;
    const image: AtlasSprite = sampler === undefined ? { src } : { src, sampler };
    return {
      image,
      sources: {
        handle(texture: string) {
          const i = textures.indexOf(texture);
          if (i < 0) throw new RangeError(`TextureAtlasBuilder: ${texture} isn't in this atlas`);
          return { ...image, atlasPick: i };
        },
      },
    };
  }
}
