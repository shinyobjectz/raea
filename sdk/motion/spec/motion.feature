@spec
Feature: @raea/motion, the behaviour of its API
  The spec the SDK is built from (clean-room: see sdk/motion/README.md). Written by the spec writer from public docs of
  React-video libraries and from the maths; builders work from this file and the conformance suite, nothing else.
  Frames are integers from 0; a composition's frame t is time t / fps seconds.

  Rule: A composition is a component rendered once per frame
    Scenario: Declaring and registering
      Given `<Composition id component width height fps durationInFrames defaultProps?>` inside the root component
      Then the id is letters, digits and hyphens, unique among the root's compositions; width, height and
        durationInFrames are positive integers and fps a positive number, or registering fails with a message naming
        the composition and the field
      And `registerRoot(Root)` names the root component the renderer starts from; calling it twice is an error
      And a root that declares no Composition (the content registered directly) is itself one composition: id "main", 30 fps, 1080x1920, durationInFrames the latest end among its
        top-level Sequences with a finite duration, else 300
      And finding the Compositions never runs a component that uses hooks: where expanding one would, the walk stops
        there and treats it as content (the SDK's own clock-reading components and any whose source shows a hook call
        aren't called; anything else that throws when called is treated as content too)
      And defaultProps are JSON-like values handed to the component as its props
    Scenario: What a component can ask
      Then `useVideoConfig()` returns {id, width, height, fps, durationInFrames, defaultProps} of the composition it is
        rendered in (a Sequence given width and height reports those to its children instead)
      And `useCurrentFrame()` returns the frame being rendered, measured from the start of the nearest enclosing
        Sequence (the composition's frame outside any Sequence)
    Scenario: AbsoluteFill
      Then `<AbsoluteFill style?>` is a box positioned absolutely over its whole parent (top, left, right and bottom 0),
        laid out as a flex column, with the given style merged over those defaults

  Rule: interpolate maps a number through keyframes
    Scenario: The mapping
      Given `interpolate(x, input, output, options?)` with input and output arrays of equal length (at least 1)
      Then input must strictly increase; otherwise it fails naming the first pair that doesn't
      And with one keyframe the result is output[0] for every x
      And inside the input range, x falls in the segment [input[i], input[i+1]]; its progress p = (x - input[i]) /
        (input[i+1] - input[i]) is passed through the easing (identity by default) and the result is output[i] +
        eased(p) * (output[i+1] - output[i])
      And `easing` is one function for every segment, or an array of exactly input.length - 1 functions, one per
        segment
    Scenario: Outside the range
      Then extrapolateLeft (below input[0]) and extrapolateRight (above the last input) each take:
        | mode     | result                                                                     |
        | extend   | the end segment's line continued (the default), easing applied to p beyond [0, 1] |
        | clamp    | the nearest end of the output                                              |
        | identity | x itself                                                                   |
        | wrap     | x folded back into the input range (modulo its width), then mapped         |
      And NaN or an infinite x fails with a message naming x

  Rule: spring is a damped harmonic oscillator sampled at frames
    Scenario: The physics
      Given `spring({frame, fps, from = 0, to = 1, config: {mass = 1, damping = 10, stiffness = 100,
        overshootClamping = false}, durationInFrames?, delay = 0, reverse = false})`
      Then the value x(t) at t = frame / fps seconds solves m x'' + c x' + k (x - to) = 0 with x(0) = from and
        x'(0) = 0 (m mass, c damping, k stiffness); with w0 = sqrt(k / m) and zeta = c / (2 sqrt(k m)):
        | case                 | x(t) - to                                                                    |
        | zeta < 1 (bouncy)    | (from - to) e^(-zeta w0 t) (cos(wd t) + (zeta w0 / wd) sin(wd t)), wd = w0 sqrt(1 - zeta^2) |
        | zeta = 1 (critical)  | (from - to) e^(-w0 t) (1 + w0 t)                                              |
        | zeta > 1 (overdamped)| (from - to) e^(-zeta w0 t) (cosh(wh t) + (zeta w0 / wh) sinh(wh t)), wh = w0 sqrt(zeta^2 - 1) |
      And frame < 0 gives from
      And with overshootClamping, once x would pass `to` (cross it in the direction of travel) the value is `to` from
        then on
    Scenario: Settling, stretching, reversing and delaying
      Then the spring has settled at the first time after which |x - to| stays below 0.005 |to - from| (the rest
        threshold; `durationRestThreshold` overrides 0.005); its natural length is that time in frames
      And durationInFrames stretches time so the settle lands on that frame: x is sampled at
        t = frame * natural / durationInFrames / fps
      And reverse plays the (stretched) curve backwards: the value at frame f is the forward value at
        length - f, so it starts at `to` and ends at `from`
      And delay holds `from` for frames 0 to delay - 1 and starts the curve at frame delay
      And they apply in that order: stretch, then reverse, then delay

  Rule: Easing curves are functions on [0, 1] with e(0) = 0 and e(1) = 1
    Scenario: The curves
      Then the Easing namespace has:
        | name          | e(p)                                                                                  |
        | linear        | p                                                                                     |
        | quad          | p^2                                                                                   |
        | cubic         | p^3                                                                                   |
        | poly(n)       | p^n                                                                                   |
        | sin           | 1 - cos(p pi / 2)                                                                     |
        | circle        | 1 - sqrt(1 - p^2)                                                                     |
        | exp           | 2^(10 (p - 1)) for p > 0, else 0                                                      |
        | back(s = 1.70158) | p^2 ((s + 1) p - s)                                                               |
        | elastic(b = 1)| 1 - cos(p pi / 2)^3 cos(p b pi): an ease-in that swings past 0 about b times (b = 0 never does) |
        | bounce        | four falling arcs, n = 7.5625, d = 2.75: n p^2 below 1/d; n (p - 1.5/d)^2 + 0.75 below 2/d; n (p - 2.25/d)^2 + 0.9375 below 2.5/d; else n (p - 2.625/d)^2 + 0.984375 |
        | bezier(x1, y1, x2, y2) | the CSS cubic-bezier through (0,0), (x1,y1), (x2,y2), (1,1), solved for y at x = p |
        | ease          | bezier(0.42, 0, 1, 1)                                                                 |
      And the modifiers turn any curve e into: in(e)(p) = e(p); out(e)(p) = 1 - e(1 - p); inOut(e)(p) = e(2p) / 2 for
        p < 1/2, else 1 - e(2 - 2p) / 2
      And the plain curves (quad, cubic, sin...) ease in; `Easing.out(Easing.cubic)` is the usual "settle into place"

  Rule: Sequence shifts time and limits it
    Scenario: Timing
      Given `<Sequence from = 0 durationInFrames = unbounded name? layout = "absolute-fill">` inside a parent whose
        frame is f
      Then its children are rendered only while from <= f < from + durationInFrames, and see frame f - from
      And offsets add through nesting: a Sequence from 60 inside one from 30 starts at the composition's frame 90
      And a negative from starts its children partway: at parent frame 0 they see frame -from
      And durationInFrames must be a positive integer or unbounded; 0 renders nothing
      And layout "absolute-fill" wraps the children in an AbsoluteFill (later Sequences paint over earlier ones);
        "none" adds no box (style and className are then an error)
      And name only labels it in tools
    Scenario: Series, Loop and Freeze
      Then `<Series>` plays its `<Series.Sequence durationInFrames offset = 0>` children back to back: each starts
        where the previous ended plus its own offset (positive leaves a gap, negative overlaps the previous); only the
        last may be unbounded; Series itself adds no box, each Series.Sequence is an absolute-fill box
      And `<Loop durationInFrames times = unbounded>` shows its children at frame f mod durationInFrames for
        times * durationInFrames frames, then nothing
      And `<Freeze frame>` shows its children as they are at that frame, whatever the parent's frame

  Rule: Media plays in step with the frame
    Scenario: Video and Audio
      Given `<Video src startFrom = 0 endAt? volume = 1 playbackRate = 1 muted = false loop = false style?>` and
        `<Audio>` with the same props but no picture
      Then at the frame f it sees (after Sequences), the source time shown or heard is (startFrom + f * playbackRate) /
        fps seconds; endAt (in source frames) ends it there
      And past the source's end (or endAt) a Video holds its last frame and Audio is silent, unless loop, which
        restarts from startFrom
      And volume is a non-negative linear gain (1 is unity, 0.5 about -6 dB, 1.78 about +5 dB), or a function of the
        media's own frame (0 at its first frame) returning one; `gainDb` (ours) gives the same in decibels and wins
        over volume when both are set
      And `fadeIn` and `fadeOut` (ours, in seconds, 0 by default) ramp the sound in from silence at its start and out to
        silence at its end, at the audio's own rate rather than per frame, so fades shorter than a frame (a cut's 12 ms)
        can be written; they multiply with volume
      And trimBefore and trimAfter are accepted as the same as startFrom and endAt; giving both names for one is an error
      And the renderer decodes the exact frame for every rendered frame: there is no drift to correct
    Scenario: The mix
      Then every Audio and every Video's sound plays at its volume (linear: 0.5 is about -6 dB) and the renderer sums
        them into one stereo mix at 48 kHz
      And `<Mix lufs = -14 ceiling = -1.5 />` (ours; once per composition) masters that sum: a single gain brings
        its integrated loudness to `lufs` (EBU R128), then a limiter keeps true peaks under `ceiling` dBTP. Without
        a Mix the sum plays as is
      And a fade is a volume function: a clip's or a bed's fade in and out are its volume ramped over those frames
        (`volume={(f) => interpolate(f, [0, 3], [0, 1], {extrapolateRight: "clamp"})}`)
    Scenario: How a Video fills its box
      Then a Video's style honours object-fit (cover, contain, fill; cover by default), object-position (two
        percentages or keywords, 50% 50% by default) and a transform scale, all acting on the decoded frame inside the
        Video's box: cover fills the box and crops the overflow, object-position picks which part of the overflow
        stays, scale zooms about the box's centre after the fit
      And `framing="face"` (ours) chooses that for the take at each clip: cover at scale 1, positioned so the
        largest face's eyes sit 38% down the box and the face is centred across, keeping the frame filled; a take
        with a small webcam tile is scaled until the tile fills the box. `punchIn` multiplies the scale (1.15 is the
        usual). object-position and scale given explicitly win over framing
    Scenario: Media imported as modules (a renderer requirement)
      Then `import take from "./media/take01.mp4"` (and .mov, .m4v, .mp3, .wav, .m4a, .png, .jpg, .webp, .gif) gives a
        URL string in the form staticFile returns, resolved from the importing file's folder, usable anywhere a src
        is; the renderer's bundle step does this
      And a missing file fails the build, naming the import line
    Scenario: Img and staticFile
      Then `<Img src>` draws an image, and the frame is not rendered until it has loaded (a failed load fails the
        render, naming src)
      And `staticFile(path)` returns the URL of a file in the space's media folder, each path segment URL-encoded
        (so #, ? and & in names survive); a path that leaves the folder is an error

  Rule: Beyond the familiar API (README point 6)
    Scenario: Time anchored to words
      Then `word(phrase, {occurrence = 1, edge = "start"})` returns the composition frame where that phrase is said,
        from the takes' transcripts mapped through the clips on the timeline; a phrase nobody says fails, naming it
      And `useWords()` returns every word heard on the timeline as {text, from, to} in the frames of the nearest
        enclosing Sequence (to exclusive), in order: each word of a clip's take that falls inside the clip, mapped
        through it; words cut out of the edit aren't there. Captions are built from it
      And matching is on words: case and punctuation are ignored, so word("Sleep study") finds "sleep study,"; edge
        "end" gives the frame after the phrase's last word ends
    Scenario: Pacing to a song's energy
      Given a music-led piece: it is mapped first (`raea-motion song FILE --plot`), the plot checked by eye, and every
        cut, slam and move timed from the map's beats, downbeats and events, never from a constant BPM
      Then `raea-motion song FILE [-o OUT.song.json] [--srt S.srt|none] [--plot OUT.png]` writes a song map (raea-perceive
        song, ai/perception/src/song.rs): beats, downbeats and bars from Beat This!, each moved below the 20 ms frame grid
        to the vertex of the parabola through its logit and its neighbours; tempo the slope of a line fitted through the
        downbeats against their bar count, over the beats per bar (never a median of quantised gaps); per beat, 0..1 over the song,
        loudness, onsets, brightness, low end, high band and a combined energy; sections from the file's `[Section]`
        markers (--srt, an .srt beside it, or its own timed-text stream) else from novelty snapped to downbeats, each
        with its energy, a level (intro, low, mid, high, outro) and cuts per bar (0.25, 0.5, 1, 2 or 4); and events
        {kind, time, end?, beat, bar, strength}: build (a rise over 2+ bars, into the drop it leads to), drop (a
        downbeat's jump or impact, `holds` false when one hit is followed by the band cutting out), stop (the low end
        gone for a few beats, with its `hits`), hit (an onset well over the same place in the bars either side), dip
        and break (a fall that holds); --plot draws `<SongPlot map>` to check it by eye
      And `songAt(map, seconds)` returns where the song is: beat, bar, beatInBar, phase, barPhase, the features
        interpolated between beats, section and sectionProgress, cutsPerBar, drop / nextDrop with sinceDrop / untilDrop
        (Infinity when there is none), build with inBuild and buildProgress 0..1, stop / inStop, dip, and the next event
      And `useSong(map, {offset = 0})` is songAt at the current frame (frame 0 of the enclosing Sequence is `offset`
        seconds into the song) with the map bound to the fps as `song`; `song(map, {fps, offset})` gives frame(s),
        seconds(f), at(f), onBeat(every, {from, to}), onDownbeat(every, range), eventsOf(kind, range) and
        cutsAt({from, to, by = "section", scale = 1, drops = true, stops = true, builds = true, minGap = fps / 4}):
        each section's (or bar's) rate on its beats, every drop, a stop's hits in place of its bars, a build's second
        half at double rate; cuts closer than minGap merge, a drop's frame kept
    Scenario: Placement from perception
      Then the Clock takes `perception = {face?: {x, y, w, h}, safe?: {top, bottom}}` for the frame, in composition px
        (perception's largest face; the platform's safe area)
      And `<Region name = "auto" margin = 0.06>` (names: auto, top, bottom, center, full) is a box positioned
        absolutely at the band it names, inset across by m = round(margin * width) on each side. With W and H the
        composition's size, top = safe.top or 0.08 H and bottom = safe.bottom or (H > W ? 0.80 : 0.92) H:
        | name   | band (from y to y)                                                                              |
        | top    | top to face.y - m, or top to 0.45 H without a face                                              |
        | bottom | face.y + face.h + m to bottom, or 0.55 H to bottom without a face                               |
        | center | 0.33 H to 0.67 H                                                                                |
        | full   | top to bottom                                                                                   |
        | auto   | the taller of top and bottom (top on a tie)                                                     |
      And a band shorter than 0.08 H renders nothing, and its box records why as data-refused ("no room top: the face
        leaves N px")
      And the Region sets CSS custom properties on its box for what's inside: --face-top, --face-bottom, --face-left,
        --face-right (when there's a face), --region-width and --region-height (px), and data-region, data-band-top,
        data-band-bottom for the renderer; `useRegion()` returns {name, x, y, w, h} inside it
    Scenario: The proof
      Then `proofBoxes({width, height, face?, parts: [{name, x, y, w, h, band?: [top, bottom]}]})` returns one fault
        string per problem, empty when all is well: a part leaving the frame ("<name> runs off the frame"), taller
        than its band ("<name> is <h> px tall and its room is <n> px"), or covering more than 8% of the face's area
        ("<name> covers <p>% of the face at x .. y .. w .. h"); the renderer measures each part after layout and
        refuses a frame with faults
    Scenario: Generated assets as values
      Then `sao(prompt, {seconds = 2})`, `fish(text, {seconds = 4, voice?})` and `lyria(prompt, {seconds = 30})` return
        an asset {kind, prompt, seconds, id, src}: id is a stable hash of kind, prompt and seconds (the same call always
        gives the same id) and src is "asset:<kind>/<id>"; `Tone.tokensynth(timbre, notes = ["C2", "C3", "C4"],
        {seconds = 2, velocity = 100, seed = 1234, precision = "fp16"})` returns one such asset per note, a Sampler's urls
      And an Audio given an asset as src carries data-generate="<kind>", data-prompt and data-seconds, so the renderer
        makes it once and caches it in the edit's assets/ by everything that decides its bytes: sao and tokensynth on
        this computer (installed on first use), fish and lyria through raea's API under the run's paid-generation cap
      And music is written as Tone.js instruments (TokenSynth notes or an open sample library's in a Sampler); sao is
        foley; fish is narration; lyria is kept for older edits and isn't what the guides teach
    Scenario: 3D in the same tree
      Then `@raea/motion/3d` (src/3d.tsx) has Canvas3D, Mesh {position = [0,0,0], rotation = [0,0,0] (radians),
        scale = 1 (or [x,y,z]), shape?}, the geometries Box {args: [w, h, d]}, Sphere {args: [r]}, Plane {args: [w,
        h]}, Cylinder {args: [radiusTop, radiusBottom, height]}, Torus {args: [radius, tube]}, MeshStandardMaterial
        {color, emissive, emissiveIntensity, metalness, roughness}, AmbientLight {intensity = 1, color = "#ffffff"},
        DirectionalLight {position, intensity = 1, color = "#ffffff"} and PerspectiveCamera {position, fov = 50}
      And they render as elements carrying their props for Bevy (data-3d = the component's name, as "Mesh" or
        "DirectionalLight"; data-props = their props as JSON); a Mesh's data-shape is its Manifold expression: its own `shape` if given, else from its geometry child:
        box(w, h, d), sphere(r), rotate(cone(radiusBottom, radiusTop, height), -90, 0, 0) for a Cylinder (standing
        along y, as three.js has it), torus(radius, tube), box(w, h, 0) for a Plane
    Scenario: 3D: the rest of a scene (sdk/motion/docs/bevy-coverage.md)
      Then `@raea/motion/3d` also has Group {position, rotation, scale} (its children's transforms composed into
        theirs), PointLight {position, intensity, color, distance, castShadow}, SpotLight {position, target, angle,
        penumbra, intensity, color, distance, castShadow}, DirectionalLight {target, castShadow}, PerspectiveCamera
        {lookAt, near, far}, MeshPhysicalMaterial {clearcoat, clearcoatRoughness, reflectance}, MeshBasicMaterial
        (unlit), material {opacity, transparent, map}, Model {src: a .glb/.gltf, animation, time (the Sequence's
        seconds by default), loop, scene}, ImagePlane {src, width, height}, the geometries Cone {args: [radius,
        height]} and Text3D {children or text, size, depth, font, weight}, and Bloom {intensity, threshold}, Fog
        {color, near, far | density} and ToneMapping {mode, exposure} inside a Canvas3D
      And Look {mode "paint"} draws the frame as flat illustration: every standard-material surface lit by the suns in
        `bands` flat tones (lit; turned away x `mid`; in a cast shadow x `shadow`, the shadow cut hard), then edges
        wandered `rag` pixels by noise seeded from `seed` (and `boil`: a new seed every `boil` frames, folded in from the
        frame by the SDK), a fixed paper `grain`, `sky` (a colour or [top, bottom]) where nothing is drawn and an optional
        `palette` every pixel snaps to; bands, grain, rag and seed are keyed per frame. Look {mode "cartoon"} is paint
        with two bands, a boil every 2 frames and an Outline {width} of `outlineWidth` pixels in `outline`
        (conformance/look.test.tsx; sdk/motion/bench/bevy/paint)
      And Fire {position (its foot), scale (a height, or [width, height, depth]), intensity, color, seed, speed, time
        (the Sequence's seconds by default), steps, opacity, light} draws a flame as a volume (engine/rdb/src/engine/fire.rs and
        fire.wgsl: rising 3D noise in a tapering envelope, a function of the row's time), with a flickering PointLight
        unless light is 0
      And Splat {src (.ply 3DGS with SH up to degree 3, .splat, .spz; a `x.ply.json` sidecar's transform places it),
        position, rotation, scale, and per-frame effects reveal (revealFrom | revealAxis, revealSoft, revealGlow),
        assemble | disperse (wind, curl), explode, wave {amplitude, wavelength, phase}, twist, morph {to, amount}, pulse
        (pulseScale, pulseGlow), grade (brightness, saturation, contrast, tint, opacity), splatScale, crop, bands, paint, seed}
        draws gaussian splats (engine/rdb/src/engine/splat.rs and splat.wgsl: EWA-projected ellipses blended back to front,
        depth-tested against the meshes, through the frame's look), sorted each frame for the frame's camera alone by a
        stable sort (ties by index), every effect a closed-form function of the row's values and seed
        (conformance/splat.test.tsx; sdk/motion/bench/bevy/splat)
      And `raea-motion splat train CLIP|DIR -o x.ply` makes a splat from footage on this Mac (tools/splat: frames,
        COLMAP through pycolmap, Brush on Metal), cached by the input's content, with a sidecar placing it +Y up in
        metres and listing the solved cameras
      And a render and a baked play draw the same rows (sdk/react/src/motion/scene3d.rs builds them, engine/rdb/src/engine/
        scene3d.rs draws them): every value that changes from frame to frame is keyed on each frame (the camera, lights,
        transforms, a material's colour, opacity, glow, roughness and metalness, a model's clip time), parts are matched
        across frames by name (or kind and shape, in tree order), and a part missing from some frames is hidden there,
        so 3D inside a Sequence appears in play when it appears in a render
      And sdk/motion/bench/bevy/check.sh compares each fixture's render with its baked play (SSIM >= 0.999)
    Scenario: 3D: matchmove (3D standing in footage's own world)
      Then `raea-motion track CLIP [--subject X,Y,W,H] [--from S] [--seconds N] [--height M] [--fov DEG] [--smooth F]
        [--dir EDIT_DIR] [-o OUT.json]` solves the clip's camera (tools/matchmove/run.py: COLMAP with one shared
        SIMPLE_RADIAL camera and sequential matching, the subject masked out of the features by its EdgeTAM matte;
        unregistered frames slerped, the path smoothed `smooth` frames) and prints an APPLIED line with the frames
        registered, the reprojection error, the fov and the camera's height; a shot with too little parallax falls back
        to rotation only (homographies) or static, and says so in `mode` and `notes`; a bad flag is REFUSED by name
      And the solve is kept in the edit's asset store (rdb Store::matchmove), keyed by the clip's content and the params,
        so `<Matchmove>` with the same params finds it; the solver installs itself on first use (tools/matchmove/setup.sh)
      And the solved world is in metres with +Y up and the ground (the RANSAC plane facing the cameras' up) at y = 0, the
        origin where frame 0's centre ray meets it, frame 0 looking down -Z, scaled so the camera stands `height` (1.6)
        metres up
      And Matchmove {src, from, seconds, subject, height, fov, smooth, solve?, crop, zoom, cutout, toneMapping} is its
        own Canvas3D: each frame draws Plate {src, time, k1} (the footage behind the 3D, undistorted by the solved k1),
        puts the PerspectiveCamera where the solve says (position, `up` for its roll, the solved fov) and draws the
        subject's cut-out as a page image over the 3D; useMatchmove() gives {solve, index, time, camera, project(p)}
      And a frame of another shape than the footage (9:16 from 16:9) sees a window of it: the footage scaled to cover the
        frame (times `zoom`), `crop` (0 left .. 1 right, or [x, y]; may change per frame) placing the window, and the
        camera's `view` {width, height, x, y} making the projection off-axis so the 3D stays locked to it
        (conformance/matchmove.test.tsx; rdb engine::matchmove tests; tools/matchmove/test_run.py)
    Scenario: Toon surfaces and ink outlines (MeshToonMaterial)
      Then `@raea/motion/3d` has MeshToonMaterial {color, steps = 2 (1-8) | bands: [{at, color?}] (darkest first),
        shadowColor (by default color darker in Oklab, its hue nudged towards violet, never grey), rim: true | {intensity,
        color, width}, highlight: true | {intensity, color, size}, outline: true | {width (pixels, 0-64), color (by
        default the colour's hue, very dark)}, emissive, emissiveIntensity, opacity, transparent, map, animate}; colours
        are hex as the other materials'; out-of-range values throw (conformance/extensions.test.tsx "3D toon")
      And inside a Mesh it is the mesh's surface; inside a Model it covers the model's glTF materials, each keeping its
        own colour and texture unless `color` is given, or only the one named by `material` (one without a name covers
        the rest); skinned and morphed meshes are posed under it and under their outline as Bevy poses them
      And it is lit by the scene's directional, point, spot and ambient lights and their shadows: a point's band is
        picked by r, the share of the light reaching it that it faces (ambient raises r everywhere), against the bands'
        starts (k / steps); a band shows its colour, unmultiplied by the light's brightness and tinted by its hue, with
        edges hard to within a pixel (anti-aliased by the screen-space change of r), so a terminator that doesn't move
        doesn't change from frame to frame
      And its `outline` is an inverted hull: the mesh drawn again with smoothed normals and its front faces culled, each
        vertex pushed out on screen by `width` pixels at the composition's size (the same width near and far) and a
        little back in depth, so it closes over hard edges and doesn't fight the surface; the hull casts and receives
        no shadows
      And `color`, `shadowColor` and `emissive` animate (an `animate` track blends them in Oklab, as the other
        materials'), with emissiveIntensity, opacity, `rimIntensity`, `highlightIntensity` and `outlineWidth`: each
        frame's value is keyed in the rows (engine/rdb/src/engine/toon.rs), so a render equals its play
      And sdk/motion/bench/bevy/toon/check.py measures the fixture's frames: three bands on a 3-step sphere, darker away
        from the light, with edges at most 2 px wide; the outline 3 px +- 1 on a near and a far sphere; no surface
        pixel touching the background (no gaps); a still sphere's pixels identical at frames 10, 30 and 50; and two
        renders of a frame byte-identical; check.sh compares its render with its baked play
    Scenario: The camera's lens (Fisheye, LensDistortion)
      Then `@raea/motion/3d` has Fisheye {strength = 1 (0..1), mode "equidistant" | "stereographic", fov (degrees the
        camera widens to at strength 1), zoom} and LensDistortion {k1, k2, zoom (by default enough to keep a barrel's
        corners filled)}, both with edges "clamp" | "black"; every number animates per frame (camera columns lens_*,
        keyed) and out-of-range values throw; a lens's vignette and colour fringes are Grade's
      And the lens is a pass over the 3D after tone mapping (engine/rdb/src/engine/lens.rs): radial, so lines through the
        centre stay straight and lines off it bow monotonically; a frame whose lens does nothing skips it, so
        strength 0 is no lens, byte for byte; the page drawn over the 3D is never bent
      And sdk/motion/bench/bevy/lens/check.py measures it (off = absent, straight centre lines within 1 px, monotonic
        curves, the page untouched, byte-identical renders), and check.sh compares its render with its baked play
    Scenario: Motion blur, and toon surfaces under the camera's effects
      Then `@raea/motion/3d` has MotionBlur {shutterAngle (0..4 of a frame, 0.5 by default), samples (1-32, 4)}: Bevy's
        MotionBlur on the camera (engine/rdb/src/engine/motion_blur.rs), on in the frames that have it (mb_on keyed per frame)
        and its shutter keyed per frame
      And with MotionBlur in the edit every update of frame f is preceded by one of frame f - 1 (Engine::pre_step, its
        readback tagged with generation 0 and passed over) and the shaders' frame counter is the frame, so a frame's
        blur is the same however it was reached (a render, a seek, play)
      And sdk/motion/bench/bevy/cinema/check.py measures it in a toon room under VolumetricFog, DepthOfField, Grade and
        AmbientOcclusion (a frame before the MotionBlur comes in = the edit without it, byte for byte; each effect
        changes the picture; MotionBlur changes only the moving part; a toon band edge stays hard under fog, grade, blur
        and AO; byte-identical renders), and check.sh compares its render with its baked play
    Scenario: Soft shadows and a high-key studio
      Then DirectionalLight, SpotLight and PointLight take softShadow: true or a size (PCSS, Bevy's
        experimental_pbr_pcss: a sun's soft_shadow_size, 0.5 by default; a bulb's radius in metres, 0.25), the light
        row's `soft` column; bias, map size and cascades stay <Shadows>'s
      And under ToneMapping mode "none" with the exposure set for the cyc, a white cyc reads near #f7f7f5 and a coloured
        product keeps its colour (bevy/lights' soft shadow notes)
      And sdk/motion/bench/bevy/studio/check.py measures it (Environment at intensity 0 = absent, its fill lifting the
        shadow, soft shadows gentler at the edge, AO darkening the contact, the high-key cyc and peel, byte-identical
        renders), and check.sh compares its render with its baked play
    Scenario: Page layers under and over the 3D (Canvas3D layered)
      Then `<Canvas3D layered>` renders an `<img data-raea-scene>` slot as its first child, and the 3D's frame is that
        slot's picture in the page's paint order (sdk/react/src/motion.rs, sdk/react/src/live.rs): page elements before the
        Canvas3D and its holder's background are under the 3D, positioned elements after it are over it, and the 3D
        clears to transparent (#00000000)
      And sdk/motion/bench/bevy/layers/check.py measures it (outside the sphere the frame is the page alone, byte for byte;
        the sphere hides the disc under it; the bar after it covers the sphere; byte-identical renders), and check.sh
        compares its render with its baked play
    Scenario: Games: state from input, frame by frame
      Then the Clock takes `input = {[frame]: {keys?: string[], pointer?: {x, y, down}}}`; `useInput()` returns this
        frame's ({keys: []} when none)
      And `useGameState(initial, step)` returns the state at this frame: initial at frame 0, and at frame f, step
        applied for each earlier frame g in order as step(state, input at g, g). Frames count from the nearest
        enclosing Sequence, like useCurrentFrame, so a game starts when its Sequence does (input is read at the
        matching composition frame).
      And the fold is computed once and kept: a state reached at an earlier frame is reused, not replayed from frame 0, so
        rendering every frame of a game in order costs a number of steps proportional to its length (a 450-frame game
        replayed from zero each frame took about 400,000 steps and stalled on the iPhone); a frame stays a function of
        its number alone, since the kept states are exactly the fold's It is a pure fold, so any frame
        renders on its own and the same input always gives the same game
      And `useGameEvents(initial, step, isEvent)` returns, in order, the frames at which isEvent(before, after) holds,
        where after is the state step makes from before at the frame before (the same fold, up to the composition's
        end), so sound can be cued to what happens in the game:
        `{hits.map((f) => <Sequence from={f}><Audio src={thump} /></Sequence>)}` (a breakout reference had to hard-code
        31 such frames from a play log)
    Scenario: The clock renderers use
      Then `<Clock frame fps width height durationInFrames id? defaultProps? words? perception? input?>` puts the tree inside it at that
        frame of a composition with those settings: useCurrentFrame, useVideoConfig, word and useWords read it. A
        renderer (Bevy's, or a golden test's) wraps a composition's component in a Clock per frame
      And Video, Audio and Img render as elements that carry their props for the renderer (a Video's source time at
        this frame already computed, as data-source-time in seconds, so the renderer needn't redo the timing), and
        Composition and registerRoot record what was registered so a renderer can list compositions
    Scenario: The test clock
      Then the main entry (@raea/motion) imports nothing that can't run inside the engine's JavaScriptCore (no server renderer, no
        Node APIs); `renderFrame` lives in the test entry, @raea/motion/test (src/test.tsx)
      And `renderFrame(node, {frame, fps, width, height, durationInFrames, words?, perception?, input?})` renders a tree at one frame
        without a window and returns what's visible, so the conformance suite can test timing without pixels; `words`
        stands in for the timeline's transcript ({text, from, to} in composition frames) so word() and useWords() can
        be tested without media
      And the test entry also has `resetRegistry()`, which forgets the registered root and compositions so one test
        process can register several

  # The engine modules below are raea's own designs, not modelled on another library: they sit outside the clean-room
  # question (README, Layout). Their behaviour is still specified here, so tests and builders work from one place.

  Rule: Native calls reach the engine (@raea/motion/native)
    Scenario: The call
      Then `native(name, args)` sends `args` as JSON to the engine's call `name` and returns its answer parsed (undefined
        for an empty answer); a call the engine refuses throws an Error carrying the engine's message
      And `hasNative()` says whether the engine is present: true inside raea's JavaScriptCore runtime (raea-motion render, run and test, the
        apps, the iPhone), false under plain bun, where `native` throws "<name> needs the raea engine ..."
      And `print(...parts)` writes one line (strings as they are, anything else as JSON) to the run's output inside the
        engine, and to the console under plain bun
    Scenario: The registry
      Then every call name the engine answers is listed once in the registry with its argument and answer types, and
        the TypeScript types for `native` (src/native-calls.ts) are generated from that list, so a call that doesn't
        exist or takes the wrong arguments fails to type-check
      And names are grouped by module with a dot: solid.*, ecs.*, vdec.*, venc.*, adec.*, frame.*, media.*, plus print
        and done

  Rule: Solids are native Manifold geometry held by handle (@raea/motion/3d `manifold`, `Solid`)
    Scenario: Making solids
      Then `manifold.box(w, h, d)`, `roundedBox(w, h, d, r)`, `sphere(r)`, `cylinder(r, h)` (along z),
        `cone(r1, r2, h)`, `torus(major, minor)` and `text3d(text, size, depth, font = "Inter", weight = 800)` each
        return a Solid, centred on the origin; a Solid is a handle (`id`) to a solid the engine holds, plus `expr`, the
        expression that rebuilds it
      And every number must be finite: anything else throws a TypeError naming the function before the engine is
        called; text3d with empty text throws
    Scenario: New solids from solids
      Then `a.union(...others)`, `a.minus(...others)` and `a.intersect(...others)` are the boolean of a with each other
        in turn, and need at least one other (else a TypeError); `a.hull(...others)` is the convex hull of all of them
        (of a alone when none)
      And `a.move(x, y, z)` translates, `a.rotate(x, y, z)` turns by degrees about x, then y, then z, and `a.scale(s)` or
        `a.scale(x, y, z = 1)` scales; each returns a new Solid and leaves a unchanged
    Scenario: Asking about a solid
      Then `volume()`, `bounds()` ({min, max} as [x, y, z]), `triangles()` and `isEmpty()` answer from the native solid,
        and `slice(z)` is the cross-section at height z as {area, contours, polygons} (a hole is its own contour)
      And the answers match closed form: box(2, 1, 0.5) has volume 1; box(2, 2, 2) minus cylinder(0.5, 3) has volume
        8 - 0.5 pi (to 0.02) and its middle slice has area 4 - 0.25 pi and 2 contours; box(2, 1, 0.5).move(0, 0, 5) has
        min z 4.75; a box minus a bigger box is empty
      And `free()` lets the engine drop the solid; a freed handle used again throws
    Scenario: The rest of Manifold
      Then `outline.circle(r, segments = 64)`, `outline.rect(w, h, centered = true)` and `outline.polygon(points)` are 2D
        outlines (contours of [x, y]; counter-clockwise solid, clockwise a hole); `manifold.extrude(outline, height,
        {divisions, twist (degrees), scaleTop})` sweeps one along z from 0, and `manifold.revolve(outline, {degrees =
        360, segments})` turns one (x >= 0) about the z axis, its y becoming z
      And `mirror(nx, ny, nz)` reflects across a plane through the origin; `transform(m)` applies a 3x4 matrix given by
        columns; `trim(nx, ny, nz, offset = 0)` keeps the normal's side of n . p = offset; `splitByPlane(...)` returns
        both sides; `split(cutter)` returns [inside it, outside it]
      And `refine(n)` cuts each triangle into n x n; `smooth({angle = 60, smoothness = 0, refine = 4})` keeps edges
        sharper than `angle` degrees and rounds the rest into a surface through the vertices; `minkowski(other)` and
        `minkowskiDiff(other)` sweep and erode by another solid
      And `mesh()` gives the triangles (positions x y z, indices by threes); `warp(fn)` moves every vertex;
        `manifold.fromMesh(verts, tris)` closes triangles into a solid; `manifold.levelSet(sdf, {min, max, resolution =
        32, level = 0})` surfaces a signed distance function (negative inside)
      And `area()` and `genus()` answer from the solid; `export(path, format?)` writes OBJ or glTF binary (.glb) and
        `manifold.import(path)` reads either back as the same solid
      And a solid no expression can describe (from a mesh, a warp, a level set, a file) is written once under its
        content's hash and its expression loads it (`obj("...")`), so a Mesh draws it like any other
      And closed form holds: extrude(rect(2, 1), 3) has volume 6; revolve of the rectangle x 1..2, y 0..1 has volume 3
        pi and genus 1; a unit box swept by a unit box is a box of 2 (volume 8); box(1, 2, 3) has area 22
    Scenario: 2D cross-sections (Manifold's CrossSection, `crossSection`, `CrossSection`)
      Then `crossSection.rect(w, h, centered = true)`, `circle(r, segments?)`, `polygon(points)`, `hull(points)` and
        `of(contours)` make a CrossSection, a value holding `polygons`, `area`, `contours` and 2D `bounds`; contours are
        unioned by the positive fill rule
      And `union`, `minus` and `intersect` take one other or more (else a TypeError); `hull(...others)`, `minkowski(other)`,
        `offset(delta, {join = "round" | "square" | "miter" | "bevel", miterLimit, segments})` (negative shrinks),
        `simplify(epsilon)`, `move(x, y)`, `rotate(degrees)`, `scale(x, y = x)`, `mirror(nx, ny)`, `warp(fn)` and
        `decompose()` each answer new CrossSections; `extrude(height, ...)` and `revolve(...)` make Solids, and
        `manifold.extrude` and `manifold.revolve` take a CrossSection wherever they take an outline
      And a solid's `crossSection(z)` (its slice) and `project()` (its silhouette on the x-y plane) are CrossSections
      And closed form holds: rect(2, 2) with rect(2, 2) moved 1 along x unions to 6 and intersects to 2; rect(2, 2)
        offset 0.5 with miter corners is 9, with round corners 9 - (4 - pi) / 4; rect(2, 2) swept by rect(1, 1) is 9;
        a circle of 1 minus a circle of 0.5 extruded 2 has volume 1.5 pi and genus 1; a sphere's projection has area pi
    Scenario: Batches and the last of Manifold
      Then `manifold.union(...solids)`, `manifold.difference(first, ...cutters)` and `manifold.intersection(...solids)`
        take two solids or more in one native call; `manifold.compose(...solids)` holds disjoint solids as one, and a
        solid's `decompose()` gives its disconnected pieces; `manifold.hull(...solids)` or `manifold.hull(points)`
      And `simplify(tolerance)`, `refineToLength(length)` and `refineToTolerance(tolerance)` re-mesh a solid;
        `minGap(other, searchLength)` is the closest distance up to searchLength; `rayCast(from, to)` lists where the
        segment meets the surface, nearest first, each {distance (0..1 along it), position, normal}
      And every one has an expression that rebuilds it (refine_length, refine_tolerance, simplify in rdb's shape
        language; pieces and point hulls written under their hash), so a Mesh draws it
    Scenario: The SDK can't be lost
      Then sdk/motion/conformance/solid.test.ts names every export of solid.ts (Solid's methods, manifold's, CrossSection's,
        crossSection's and outline's) and fails when one is removed, or added without a test that calls it; the
        @raea/motion/3d entry exports manifold, Solid, CrossSection, crossSection and outline
      And sdk/react/bench/timelines/solids draws a solid built with each group of operations (constructors, booleans,
        transforms, cutting, surfaces, hulls and Minkowski, 2D, 3D to 2D, meshes), and its render equals its baked play
        and seeks (SSIM >= 0.999); bench/ci.sh runs both (gates motion:solid and timelines)
    Scenario: Solids in the tree
      Then a `<Mesh shape={solid}>` carries the solid's expr as its data-shape, so the renderer rebuilds the same
        geometry (golden case 05 renders the same frames as case 04's expression)

  Rule: A compiled-in Bevy plugin is driven by generic rows and stepped by the frame (sdk/motion/docs/bevy-plugins.md)
    Scenario: Components and resources by type path
      Then `<BevyComponent type value from?>` inside a Mesh becomes a `component` row on that mesh's row, and
        `<BevyResource type value>` inside the Canvas3D a `resource` row; type is a full reflected type path and value
        the JSON Bevy's reflect serde reads (a struct an object, a newtype its inner value, an enum "Unit" or
        {Variant: ...}, a Vec3 [x, y, z]); a value that changes from a frame on is a new row from that frame
      And rdb applies each through the TypeRegistry (TypedReflectDeserializer, then ReflectComponent::insert): a struct
        with a default may leave fields out, one without must be whole (else an error, never a panic); an unknown type
        is an error naming it; an edit with these rows in a build with no plugin is refused
      And `<Mesh driven>` gives the mesh's Transform to the plugin after the first frame
    Scenario: The frame clock
      Then with a plugin installed, an update that draws frame f after frame f - 1 advances Bevy's Time by 1/fps (and
        FixedUpdate once), any other update by zero; a frame reached by a jump steps through the frames between; a frame
        before the last stepped starts a new App from the load and steps from 0
      And so a plugin that keeps the rules (Time, never the wall clock; state changed only on stepping updates; seeded
        randomness) renders as it plays and seeks: sdk/motion/bench/bevy-plugins/check.sh (avian3d SSIM 1.0, bevy_hanabi
        0.9999 with its blended half the only difference)
    Scenario: TypeScript generated from the registry
      Then `raea-bevy-sdk` (react's plugin build) walks the App's TypeRegistry for a plugin's components and resources
        and writes a type for each type their fields reach and one typed wrapper per component and resource; a type
        reaching an asset handle or an entity is marked as not drivable from TSX alone

  Rule: An ECS world steps frame by frame (@raea/motion/ecs)
    Scenario: Entities and columns
      Then `createWorld()` makes a world whose tables are the engine's when the engine is present and TypeScript's
        under plain bun (`world.native` says which); `createWorld({native: false})` always gives TypeScript's, and
        `{native: true}` without the engine throws
      And `spawn(kind, cols = {})` adds an entity of `kind` with columns (numbers, strings, booleans) and returns its
        id, counting up from 1 in each world; `get(id)` returns {id, kind, ...cols}; `set(id, cols)` merges columns
        (id and kind can't change); `despawn(id)` removes it; get, set or despawn of an id that isn't there throws
        "no entity <id>"
      And `count(kind)`, `column(kind, col)` (that numeric column in id order, null where unset or not a number) and
        `rows()` (every row, by kind then id) read the tables
    Scenario: Queries
      Then `query(kind, where = [])` returns the ids of kind's entities that pass every [column, op, value] condition,
        in id order; ops are == != < <= > >=; booleans compare as 0 and 1; < <= > >= hold only between numbers
    Scenario: Systems and time
      Then `system(fn)` adds a system run as fn(world, frame) on every frame, after the ones added before it, and
        `step(frame)` runs them once
      And `at(frame)` returns a copy of the world after frames 0 .. frame-1 have run from the world as built (the world
        itself is the state before frame 0); asking for the same frame again gives the same world, and a frame is a
        function of its number alone, as any frame of a video must be
      And spawning or adding a system invalidates what at() had worked out; frame must be an integer from 0, else a
        RangeError
      And the engine's tables and TypeScript's give the same answers for the same calls
    Scenario: Bevy's world: components, queries, hierarchy, resources and events
      Then in the engine a world is Bevy's own World: each component name is a Bevy component registered at run time
        (its value a JSON object, kept in Bevy's archetype tables), and a kind (above) is a component named after it
      And `add(components, {parent?})` makes an entity with any number of components ({Position: {x, y}, Ball: {}});
        `insert(id, components)` adds or replaces, `remove(id, ...names)` takes off, `components(id)` and
        `component(id, name)` read (undefined when it hasn't it)
      And `find({with, without, where})` returns, in id order, the entities with every `with` component and none of the
        `without`, passing every [column, op, value] condition, a column "Comp.field" or a field of the first `with`
        (Bevy's query in the engine)
      And `setParent(id, parent | null)`, `parent(id)` and `children(id)` are Bevy's parent and child; despawning a
        parent despawns its children
      And `resource(name, value?)` sets or reads a value for the whole world (null when unset); `send(name, value)`
        posts an event that the systems after the sender see this frame through `events(name)`; every frame starts
        with no events
    Scenario: A world drawn by the engine
      Then `<Entities world={w.at(frame)} />` (in Canvas3D) draws every entity with a Mesh component ({shape, color,
        roughness, metalness}) and a Transform ({x, y, z, rx, ry, rz in radians, scale}) as a mesh, with no React
        element per entity; a child's Transform is in its parent's space (golden case 06 matches the same meshes
        placed by hand)

  Rule: State machines, step by step (@raea/motion/machine; games through @raea/motion/games)
    # raea's own design after Bevy's seldom_state, in XState's config shape (sdk/react/docs/state-machines.md)
    Scenario: A machine is plain data
      Given `machine<C, E>()({ id, initial, states, key?, strict? })`, each state optionally holding entry, exit and tick
        actions, always (transitions polled in order), after ({steps: transitions}), on ({event: transitions}), choose
        (candidates for a decider), initial and states (substates), history ("shallow" | "deep"), onDone and type: "final"
      Then a transition is a state name, or {to | target, when | guard?, after?, actions?, description?}; actions are pure
        (ctx, env) => ctx; state names are unique across the machine
      And the state names are inferred from the definition: a transition, an initial or a query naming a state that
        doesn't exist, or a guard reading a field C or E lacks, fails to compile
      And building it throws, naming the machine, for: a compound state without an initial among its substates, a
        top-level initial that isn't top-level, two states with one name, a final state with substates, transitions or
        tick actions, an after key that isn't a whole number, and (unless strict is false) a state no transition
        reaches or a non-final state with no transition out (its own or an enclosing state's)
    Scenario: Stepping
      Then the snapshot is plain JSON at ctx[key] ("sm"): {path, at, n, e, by?, hist?}; `start()` gives the snapshot before
        the first step, and the first step enters the initial state and its initial substates (entry actions outer
        first), then ticks
      And each later step: n counts up; the active states are polled innermost first, and within a state `on` edges for
        this step's events, then always, after and onDone (a compound's, once its active substate is final) in their
        order, then choose; an edge is enabled when the state has been active at least its `after` steps and its guard
        holds; the first enabled edge fires, at most one a step
      And a transition leaves the active states below the kept depth innermost first (exit actions), runs its actions,
        then enters the target's line and initial (or remembered, with history) substates outer first; the kept states
        are those up to the source when the target is inside it, else up to the common ancestor, so a target that is the
        source or encloses it is left and entered again
      And after the transition every active state's tick actions run, outer first; the step returns a new context and
        leaves the old one unchanged
      And `state(ctx)`, `path(ctx)`, `is(ctx, name)` (any active state), `steps(ctx, name?)` (-1 when inactive) and
        `entered(before, after, name?)` read the snapshot; before the first step they report the initial leaf
    Scenario: Deciders and the trace
      Then at a choice point the enabled `choose` candidates go to the step's decider ({name, decide(point)}), which
        returns an edge among them, or null to wait; picking any other edge throws; without a decider firstDecider picks
        the first; replayDecider(log) replays decisions recorded per machine and step
      And given `trace`, a step reports each transition as a row {machine, step, from, to, edge, kind, by, evidence?}: by
        is "start", "guard" or the decider's name; `transitionOf(m, before, after)` recovers the same row from two
        snapshots (without evidence)
    Scenario: Games
      Then a game steps its machines once a frame inside its fold, so steps are frames; `stepAll(m, entities, env)` steps
        one machine per entity in order, and `keyEdges(prev, now)` gives "keydown:K" and "keyup:K" events
      And the same input timeline gives the same states frame by frame, inside useGameState too, whether frames are
        rendered in order, out of order, or refolded from the fold's checkpoints; a fold resumed from a JSON copy of any
        state continues identically
      And `Playtest.machines(state)` names each machine in a state by entity ({entity: m.watch(ctx)}); the playtest's
        trace records every transition as a row, and its rules coversStates, coversTransitions, noSoftlock, noUnreachable
        and reaches compile to check-pack rules (eye "rule", with args) answered from the trace with no model call

  Rule: Media through mediabunny with the engine's coders (@raea/motion/codecs)
    Scenario: The coders
      Then `useNativeMedia()` registers the engine's coders with mediabunny once (calling it again does nothing):
        a video decoder for avc and hevc, a video encoder for avc, and an audio decoder for aac, mp3 and flac; each
        says it supports its codecs only when the engine is present
      And a decoded video frame stays in the engine: the sample holds a NativeFrame (NV12, its width and height, bt709
        limited range), and asking it for pixel planes or an RGB copy throws, naming the native steps that take it;
        `frameOf(sample)` returns that NativeFrame, or null for a sample holding pixels
      And decoded audio is planar f32 laid end to end from the first packet's time, so a gap in the file's timestamps
        closes up as ffmpeg's decoder closes it, and the two agree sample for sample (the encoder's priming before 0
        dropped, as ffmpeg drops it)
    Scenario: Files and native steps
      Then `fileInput(path)` is a mediabunny Input reading the file through the engine, by range; `saveTarget(target,
        path)` writes a finished BufferTarget (throws if not finalized); `savePng(sample, path)` writes an engine frame
        as PNG; `nativeScale(w, h)` is a Conversion process step scaling engine frames natively
      And the video encoder scales a frame natively when its size differs from the encoder's, and refuses samples that
        don't hold engine frames
    Scenario: Heavy jobs go whole to the engine
      Then `transcode(path, out, {from?, to?, height?, kbps?})` trims, scales (never up; the width keeps the shape) and
        encodes with sound to MP4 natively, nothing passing through JavaScript, and returns {frames, width, height, fps,
        kbps, bytes}
    Scenario: The proof
      Then the same conversions done through mediabunny, through rdb ffmpeg and through real ffmpeg give the same
        answers (sdk/motion/tools/media-vs-ffmpeg.sh): frame counts, sizes, SSIM 1.0 for remuxes, audio correlation
        above 0.999
    Scenario: AAC out, captions in, and what the hardware encodes
      Then `NativeAudioEncoder` (registered by useNativeMedia) encodes AAC-LC with AudioToolbox, 1024 frames a packet,
        its AudioSpecificConfig as the decoder description; the encoder's priming (2112 frames) is in the first
        packets' timestamps, so the decoded sound lines up with the input (a Conversion's AAC decodes in ffmpeg with
        correlation 1.0 against the source)
      And decoded AAC starts at time 0: the source's own priming is dropped, as ffmpeg drops it
      And `wordsToWebVTT(words, fps, {perCue = 4, gap = 8})` turns the transcript's words into WebVTT cues (a new cue
        after perCue words, a pause over gap frames, or punctuation), for mediabunny's TextSubtitleSource; a Matroska
        output carries them as S_TEXT/WEBVTT
      And `hardwareEncodes()` lists the video codecs this machine encodes in hardware (VideoToolbox's list); the
        encoder offers H.264 where it's there, and AV1 or VP9 only where a chip encodes them
    Scenario: Helpers
      Then `toBase64(bytes)` and `fromBase64(text)` convert bytes and standard base64 both ways (a bare JavaScript engine has no btoa),
        padding included, and round-trip any bytes
