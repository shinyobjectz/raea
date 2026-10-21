// TONES: pure sines started, cut mid-wave, overlapped, ramped and stopped by the timeline, for the click check
// (sdk/react/bench/games/audio.sh): a sine is smooth, so any step the detector finds is one the mixer made. The tones are
// made by the harness (tone-440.wav, tone-660.wav: 4 s each, generated, not committed).
import { AbsoluteFill, Audio, Composition, Sequence, registerRoot } from "@raea/motion";

function Tones() {
  return (
    <AbsoluteFill style={{ background: "#101418" }}>
      {/* a start from silence and an end into it */}
      <Sequence from={0} durationInFrames={60}>
        <Audio src="tone-440.wav" />
      </Sequence>
      {/* the same tone again, joined mid-wave (startFrom 17 frames in): a cut */}
      <Sequence from={60} durationInFrames={60}>
        <Audio src="tone-440.wav" startFrom={17} />
      </Sequence>
      {/* a second tone over the first */}
      <Sequence from={90} durationInFrames={75}>
        <Audio src="tone-660.wav" volume={0.6} />
      </Sequence>
      {/* a gain ramp, frame by frame */}
      <Sequence from={170} durationInFrames={70}>
        <Audio src="tone-440.wav" volume={(f: number) => Math.min(1, f / 35)} />
      </Sequence>
      {/* a stop and a start 5 frames later */}
      <Sequence from={245} durationInFrames={40}>
        <Audio src="tone-660.wav" startFrom={40} />
      </Sequence>
    </AbsoluteFill>
  );
}

registerRoot(() => <Composition id="tones" component={Tones} width={640} height={360} fps={30} durationInFrames={300} />);
