@spec
Feature: A project moving to raea from another React video tool keeps its names and its CLI habits (#175, #176)
  Public #176 and #175 (@Ragulsundaram, moving a project to raea, 11-18): everything else they use was there
  (Sequence, Series, Loop, Freeze, spring, measureSpring, delayRender, staticFile), and these were missing. Each
  scenario says what the name does, as the issue describes it; raea's code is its own, and `random`'s numbers are
  raea's.

    @done
    Scenario: MV1. random(seed): a number in [0, 1) from a string or a number seed, the same on every frame and run
      Given `random("r" + i)` in a composition
      Then it is the same on frames 0 and 99, on a second run and on another machine, in [0, 1), and
        `random("a") !== random("b")`; a number seeds as its text, and `random(null)` is `Math.random()`
      And 11-20, built: sdk/raea/src/random-colors.ts, exported from `raea`; FNV-1a over the UTF-16 units and murmur3's
        finaliser. sdk/raea/conformance/random-colors.test.ts pins three values (a change to the hash moves every
        seeded scatter) and checks 2,000 seeds are in range and 1,000 are distinct with a mean within 0.05 of 0.5.

    @done
    Scenario: MV2. interpolateColors(input, inputRange, outputRange): interpolate for CSS colours, as rgba()
      Given `interpolateColors(5, [0, 10], ["#ff0000", "#0000ff"])`
      Then it is "rgba(128, 0, 128, 1)", the ends give the end colours exactly, and outside the range it holds them
      And names, #rgb(a), rgb(a)() and hsl(a)() parse (@raea/ui's canvas parser), alpha mixes, and a colour that
        doesn't parse throws naming it
      And 11-20, built: sdk/raea/src/random-colors.ts on `interpolate` per channel, clamped unless `options` says
        otherwise; the conformance test checks the issue's values, alpha, three keyframes and the error.

    @done
    Scenario: MV3. <OffthreadVideo> is <Video>
      Given `<OffthreadVideo src={staticFile("a.mp4")} />`
      Then it draws what `<Video>` with the same props draws
      And 11-20, built: the same component under both names (the conformance test checks they're one value).

    Scenario: MV4. Props from the command line: --props, getInputProps() and a Composition's calculateMetadata
      Given a composition with `defaultProps={{n: 1}}` and `calculateMetadata: ({props}) => ({durationInFrames:
        props.frames})`
      When it is rendered with `--props '{"n": 2, "frames": 90}'`, or `--props props.json`
      Then it draws 2, `getInputProps().n` is 2 anywhere in the tree, and the render is 90 frames
      And the props reach every place a composition is run (render, play, check, the app's preview), and the render's
        manifest keeps them, so a check of the file knows what it was rendered with
      And `--input` stays a game's input timeline
      And the flag goes in with its manual entry and the CLI reference check in the same change (a new flag in the
        shared build fails every lane's manual reference until it lands)

    Scenario: MV5. --scale S draws a composition at S times its size, laid out at its own
      Given a 1000x1000 composition with a 200x200 box at (0, 0)
      When it is rendered with `--scale 0.5`
      Then the file is 500x500 and the box covers (0, 0) to (100, 100), its edges drawn at the output's size (text,
        borders and images at 500 px, not a 1000 px frame scaled down)
      And above 1 for stills (a 2x still of a 1080p composition is 3840x2160)
      And 3D cameras render at the scaled size, screen-space effects (bloom's radius, outlines, grain) keep their look,
        media decode at the size drawn, and the manifest says the scale, so `raea check` judges text sizes and safe
        areas at the composition's size
      And the flag goes in with its manual entry, as MV4's
