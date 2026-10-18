# Assets: staticFile, module imports, generated sound

## What to do

- Name files in the object's media folder with `staticFile(path)`. The path is relative to `media/`:
  `staticFile("takes/take01.mp4")` gives `/media/takes/take01.mp4`, with each segment URL-encoded so names with `#`,
  `?`, `&` or spaces survive. Use the result anywhere a `src` goes (`Video`, `Audio`, `Img`).
- Or import a file as a module, from the composition's own folder:
  `import take from "./media/takes/take01.mp4"` gives the same kind of URL string. This works for .mp4, .mov, .m4v,
  .mp3, .wav, .m4a, .png, .jpg, .webp and .gif. A missing file fails the build and names the import line, which
  catches a wrong name before anything renders.
- The brand's styles come from `./styles.gen` (`<style>{css}</style>`), and `brand/brand.json` imports as JSON
  (`motion/brand`).
- Describe generated sound as values: `lyria(prompt, {seconds})` for music and `sao(prompt, {seconds})` for foley.
  Give them to `<Audio src>` (`motion/sound`).
- Show stills with `<Img src>`. The frame waits for the image to load, and a missing one fails the render, naming
  `src`.
- Keep pictures real: the person's footage, stock already in the object's media, or things you build (components,
  3D). Logos come from `media/brand/` as `raea brand` saved them.

## Why

The renderer serves the object's media folder and nothing else, so a path that leaves it (`../secret.mp4`) is an
error and so is a URL (`https://...`). `staticFile` checks both. A module import resolves at build time, so a typo
fails loudly with the line that has it instead of rendering a black clip.

Generated imagery is banned outright (floor): no AI-generated images or video, ever. Generated sound is allowed by
policy: Lyria for music, SAO for foley, Fish for narration over the picture. Narration arrives as a file in media.

## Common mistakes

- `staticFile("media/take01.mp4")`. The path is already inside media, so this points at `/media/media/take01.mp4`.
  Write `staticFile("take01.mp4")`.
- `staticFile("https://...")` or a remote `src`: refused. Put the file in the object's media first.
- `staticFile("../brand/brand.json")`: it leaves the media folder and throws. Import it as a module instead.
- Encoding the name yourself (`staticFile("my%20take.mp4")`). staticFile encodes it, so this double-encodes.
- An `<Img>` of a file that isn't there: the render fails. Check the name against the media folder.
- A logo redrawn as an SVG path or typed as text. Use the brand's logo file.
- Passing a `lyria()` asset to a `<Video>`. Generated assets are sound, for `<Audio>` only.

## Example: footage, a logo and a sound by import and by path

```tsx
import { AbsoluteFill, Audio, Composition, Img, Sequence, Video, registerRoot, sao, staticFile } from "@raea/motion";
import { css } from "./styles.gen";
import broll from "./media/broll/desk.mp4";

const LOGO = staticFile("brand/logo-light.png"); // in media/brand/
const TAP = sao("soft muted wooden click, single short tap, dry", { seconds: 0.5 });

export default function Assets() {
  return (
    <AbsoluteFill style={{ backgroundColor: "var(--brand-background)" }}>
      <style>{css}</style>
      <AbsoluteFill style={{ overflow: "hidden" }}>
        <Video src={broll} muted style={{ position: "absolute", width: "100%", height: "100%", objectFit: "cover" }} />
      </AbsoluteFill>
      <Sequence from={20} name="logo">
        <Img src={LOGO} style={{ position: "absolute", left: 390, top: 260, width: 300 }} />
      </Sequence>
      <Sequence from={20} name="tap" layout="none">
        <Audio src={TAP} gainDb={-6} />
      </Sequence>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="assets" component={Assets} width={1080} height={1920} fps={30} durationInFrames={60} />);
```
