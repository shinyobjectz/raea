// <Splat> (spec/motion.feature, "3D: gaussian splats"): its props checked and flattened into what the rows key
// (engine/rdb/src/engine/splat.rs reads them): morph and wave into their columns, assemble as a dust scatter run backwards,
// a reveal axis making the reveal a plane, colours as hex. Every value a function of the frame.
import { describe, expect, test } from "bun:test";
import { Canvas3D, Splat } from "../src/3d";
import { renderFrame } from "../src/test";
import { useCurrentFrame } from "../src/clock";

const base = { fps: 30, width: 1920, height: 1080, durationInFrames: 60 };
const props = (html: string) => {
  const m = html.match(/data-3d="Splat" data-props="([^"]*)"/);
  return m ? JSON.parse(m[1].replace(/&quot;/g, '"')) : undefined;
};
const at = (el: any, frame = 0) => props(renderFrame(<Canvas3D>{el}</Canvas3D>, { ...base, frame }));

describe("Splat", () => {
  test("a splat carries its file and placement", () => {
    const p = at(<Splat src="/media/scene.ply" position={[1, 2, 3]} rotation={[0, 1, 0]} scale={2} />);
    expect(p).toMatchObject({ src: "/media/scene.ply", position: [1, 2, 3], rotation: [0, 1, 0], scale: 2 });
  });

  test("morph and wave flatten into their columns", () => {
    const p = at(<Splat src="a.ply" morph={{ to: "b.ply", amount: 0.4 }} wave={{ amplitude: 0.2, phase: 1 }} />);
    expect(p).toMatchObject({ morphTo: "b.ply", morph: 0.4, waveAmplitude: 0.2, wavelength: 1, wavePhase: 1 });
    expect(p.wave).toBeUndefined();
  });

  test("assemble is a dust scatter run backwards; disperse is the wind's", () => {
    expect(at(<Splat src="a.ply" assemble={0.25} />)).toMatchObject({ disperse: 0.75, scatter: "dust" });
    expect(at(<Splat src="a.ply" disperse={0.25} wind={[0, 0, 1]} />)).toMatchObject({ disperse: 0.25, scatter: "wind", wind: [0, 0, 1] });
  });

  test("a reveal axis sweeps a plane; colours go as hex", () => {
    const p = at(<Splat src="a.ply" reveal={0.5} revealAxis={[0, 1, 0]} tint="red" glowColor="rgb(0, 0, 255)" />);
    expect(p).toMatchObject({ reveal: 0.5, revealMode: "plane", tint: "#ff0000", glowColor: "#0000ff" });
    expect(at(<Splat src="a.ply" reveal={0.5} />).revealMode).toBeUndefined();
  });

  test("animated props are the frame's", () => {
    function S() {
      const f = useCurrentFrame();
      return <Splat src="a.ply" disperse={f / 59} pulse={f % 2} />;
    }
    expect(at(<S />, 0)).toMatchObject({ disperse: 0, pulse: 0 });
    expect(at(<S />, 59)).toMatchObject({ disperse: 1, pulse: 1 });
  });

  test("bad values throw, naming the prop", () => {
    const r = (el: any) => () => at(el);
    expect(r(<Splat src="" />)).toThrow(/src/);
    expect(r(<Splat src="a.ply" reveal={1.5} />)).toThrow(/reveal/);
    expect(r(<Splat src="a.ply" disperse={0.2} assemble={0.2} />)).toThrow(/not both/);
    expect(r(<Splat src="a.ply" position={[0, 0] as any} />)).toThrow(/position/);
    expect(r(<Splat src="a.ply" morph={{ to: "b.ply", amount: 2 }} />)).toThrow(/morph/);
    expect(r(<Splat src="a.ply" wave={{ amplitude: 0.1, wavelength: 0 }} />)).toThrow(/wavelength/);
    expect(r(<Splat src="a.ply" tint="var(--x)" />)).toThrow(/tint/);
    expect(r(<Splat src="a.ply" splatScale={-1} />)).toThrow(/splatScale/);
  });
});
