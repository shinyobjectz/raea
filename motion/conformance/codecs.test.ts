// @raea/motion/codecs (spec: "Media through mediabunny with the engine's coders"). The helpers and refusals run
// anywhere; decoding, encoding and the heavy jobs run where the engine is, on media/fixtures, against what real ffmpeg
// reports for the same files (the numbers below are ffprobe's and ffmpeg's).
import { afterEach, describe, expect, test } from "bun:test";
import { AudioSampleSink, BufferSource, BufferTarget, Conversion, EncodedPacketSink, EncodedVideoPacketSource, Input, ALL_FORMATS, MkvOutputFormat, Mp4OutputFormat, Output, TextSubtitleSource, VideoSampleSink, VideoSample } from "mediabunny";
import { fileInput, frameOf, fromBase64, NativeAudioDecoder, NativeAudioEncoder, NativeFrame, NativeVideoDecoder, NativeVideoEncoder, nativeScale, savePng, saveTarget, toBase64, transcode, useNativeMedia, wordsToWebVTT, hardwareEncodes } from "../src/codecs";
import { hasNative, native } from "../src/native";

const engine = hasNative();
// the repo root: RAEA_ROOT in the engine (raea-motion test sets it; the iPhone probe points it at its copy)
const root = () => (native("host.env", { name: "RAEA_ROOT" }) as string | null) ?? ".";
const fixture = (f: string) => `${root()}/media/fixtures/${f}`;
const tmp = (f: string) => `${(native("host.env", { name: "TMPDIR" }) as string | null) ?? "/tmp"}/raea-codecs-${f}`;

describe("codecs: helpers", () => {
  test("base64 both ways, with padding, for any bytes", () => {
    expect(toBase64(new Uint8Array([]))).toBe("");
    expect(toBase64(new Uint8Array([102]))).toBe("Zg==");
    expect(toBase64(new Uint8Array([102, 111]))).toBe("Zm8=");
    expect(toBase64(new Uint8Array([102, 111, 111]))).toBe("Zm9v");
    expect(Array.from(fromBase64("Zm8="))).toEqual([102, 111]);
    const all = new Uint8Array(256).map((_, i) => i);
    expect(Array.from(fromBase64(toBase64(all)))).toEqual(Array.from(all));
  });
  test("a NativeFrame keeps its pixels in the engine", () => {
    const f = new NativeFrame(1, 64, 32);
    expect(f.getFormat()).toBe("NV12");
    expect([f.getCodedWidth(), f.getCodedHeight()]).toEqual([64, 32]);
    expect(() => f.getDataPlanes()).toThrow(/stays in the engine/);
    expect(() => f.toRgbSample()).toThrow(/stays in the engine/);
  });
  test("frameOf is null for a sample holding pixels", () => {
    const s = new VideoSample(new Uint8Array(4 * 2 * 2), { format: "RGBA", codedWidth: 2, codedHeight: 2, timestamp: 0 });
    expect(frameOf(s)).toBeNull();
    s.close();
  });
  test("the coders support their codecs only with the engine", () => {
    expect(NativeVideoDecoder.supports("avc")).toBe(engine);
    expect(NativeVideoDecoder.supports("hevc")).toBe(engine);
    expect(NativeVideoDecoder.supports("vp9")).toBe(false);
    expect(NativeVideoEncoder.supports("avc")).toBe(engine);
    expect(NativeVideoEncoder.supports("hevc")).toBe(false);
    expect(NativeAudioDecoder.supports("aac")).toBe(engine);
    expect(NativeAudioDecoder.supports("opus")).toBe(false);
  });
  test("useNativeMedia registers once", () => {
    useNativeMedia();
    useNativeMedia();
  });
  test.skipIf(engine)("without the engine, files say they need it", () => {
    expect(() => fileInput("x.mp4")).toThrow(/needs the raea engine/);
  });
});

/** Waits (up to 2 s) until no native coder or frame is alive: mediabunny closes a sink's decoder a moment after its
 * last sample, and a phone has few hardware decoder sessions, so each test leaves none behind or fails. */
async function nothingAlive() {
  let live = native("media.live", {});
  for (let i = 0; i < 100 && live.vdecs + live.vencs + live.adecs + live.frames > 0; i++) {
    await new Promise((r) => setTimeout(r, 20));
    live = native("media.live", {});
  }
  expect([live.vdecs, live.vencs, live.adecs, live.frames]).toEqual([0, 0, 0, 0]);
}

describe.skipIf(!engine)("codecs: the engine's coders", () => {
  afterEach(nothingAlive);
  test("an H.264 file read through the engine: track facts as ffprobe has them", async () => {
    useNativeMedia();
    const input = fileInput(fixture("take-h264.mp4"));
    const v = (await input.getPrimaryVideoTrack())!;
    expect(v.codec).toBe("avc");
    expect([v.displayWidth, v.displayHeight]).toEqual([1080, 1920]);
    expect((await v.computePacketStats()).packetCount).toBe(769);
    expect(await input.computeDuration()).toBeCloseTo(25.633, 2);
  });
  test("decoded frames stay engine frames (NV12), one a frame time", async () => {
    useNativeMedia();
    const v = (await fileInput(fixture("take-h264.mp4")).getPrimaryVideoTrack())!;
    let n = 0;
    for await (const s of new VideoSampleSink(v).samples(0, 1)) {
      const f = frameOf(s);
      expect(f).not.toBeNull();
      expect([f!.width, f!.height]).toEqual([1080, 1920]);
      if (n === 0) {
        savePng(s, tmp("f0.png"));
        expect(native("media.size", { path: tmp("f0.png") })).toBeGreaterThan(1000);
      }
      s.close();
      n++;
    }
    expect(n).toBe(30);
  }, 60000);
  test("HEVC decodes too", async () => {
    useNativeMedia();
    const v = (await fileInput(fixture("take-hevc.mov")).getPrimaryVideoTrack())!;
    expect(v.codec).toBe("hevc");
    let n = 0;
    for await (const s of new VideoSampleSink(v).samples(0, 0.5)) {
      expect(frameOf(s)).not.toBeNull();
      s.close();
      n++;
    }
    expect(n).toBe(15);
  }, 60000);
  test("AAC decodes end to end to ffmpeg's sample count", async () => {
    useNativeMedia();
    const a = (await fileInput(fixture("music-bed.m4a")).getPrimaryAudioTrack())!;
    expect([a.codec, a.sampleRate, a.numberOfChannels]).toEqual(["aac", 44100, 1]);
    let frames = 0;
    for await (const s of new AudioSampleSink(a).samples()) {
      frames += s.numberOfFrames;
      s.close();
    }
    expect(frames).toBe(1764352);
  }, 60000);
  test("a Conversion: trimmed, scaled natively, encoded by VideoToolbox, read back", async () => {
    useNativeMedia();
    const out = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    const c = await Conversion.init({
      input: fileInput(fixture("take-h264.mp4")),
      output: out,
      trim: { start: 1, end: 2 },
      video: { codec: "avc", forceTranscode: true, process: nativeScale(270, 480), processedWidth: 270, processedHeight: 480 },
      audio: { discard: true },
    });
    await c.execute();
    const buf = (out.target as BufferTarget).buffer!;
    saveTarget(out.target as BufferTarget, tmp("trim.mp4"));
    expect(native("media.size", { path: tmp("trim.mp4") })).toBe(buf.byteLength);
    const back = new Input({ source: new BufferSource(buf), formats: ALL_FORMATS });
    const v = (await back.getPrimaryVideoTrack())!;
    expect([v.codec, v.displayWidth, v.displayHeight]).toEqual(["avc", 270, 480]);
    expect((await v.computePacketStats()).packetCount).toBe(30);
  }, 120000);
  test("the encoder refuses samples holding pixels", async () => {
    const enc = new NativeVideoEncoder();
    (enc as unknown as { config: unknown }).config = { codec: "avc", width: 2, height: 2 };
    const s = new VideoSample(new Uint8Array(16), { format: "RGBA", codedWidth: 2, codedHeight: 2, timestamp: 0 });
    expect(() => enc.encode(s)).toThrow(/encodes engine frames/);
    s.close();
  });
  test("a heavy job goes whole to the engine", () => {
    const r = transcode(fixture("take-h264.mp4"), tmp("proxy.mp4"), { from: 0, to: 1, height: 480 });
    expect(r.frames).toBe(30);
    expect([r.width, r.height]).toEqual([270, 480]);
    expect(r.fps).toBe(30);
    expect(r.bytes).toBe(native("media.size", { path: tmp("proxy.mp4") }));
  }, 120000);
});

describe("codecs: captions as WebVTT", () => {
  test("words grouped into cues, timed in frames", () => {
    const vtt = wordsToWebVTT([{ text: "Hello", from: 0, to: 10 }, { text: "there,", from: 12, to: 20 }, { text: "friend", from: 21, to: 30 }, { text: "later", from: 60, to: 70 }], 30);
    expect(vtt).toBe("WEBVTT\n\n00:00:00.000 --> 00:00:00.667\nHello there,\n\n00:00:00.700 --> 00:00:01.000\nfriend\n\n00:00:02.000 --> 00:00:02.333\nlater\n");
  });
});

describe.skipIf(!engine)("codecs: AAC out, subtitles in (engine)", () => {
  test("a Conversion re-encodes the sound to AAC natively, aligned with the source", async () => {
    useNativeMedia();
    expect(NativeAudioEncoder.supports("aac")).toBe(true);
    const out = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    const c = await Conversion.init({ input: fileInput(fixture("music-bed.m4a")), output: out, trim: { start: 0, end: 5 }, audio: { codec: "aac", forceTranscode: true } });
    await c.execute();
    const back = new Input({ source: new BufferSource((out.target as BufferTarget).buffer!), formats: ALL_FORMATS });
    const a = (await back.getPrimaryAudioTrack())!;
    expect([a.codec, a.sampleRate, a.numberOfChannels]).toEqual(["aac", 44100, 1]);
    expect(await back.computeDuration()).toBeCloseTo(5, 1);
  }, 120000);
  test("a Matroska file with a WebVTT subtitle track beside the video (packets remuxed, captions added)", async () => {
    useNativeMedia();
    // a short H.264 video first
    const first = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    const c = await Conversion.init({ input: fileInput(fixture("take-h264.mp4")), output: first, trim: { start: 0, end: 2 }, video: { codec: "avc", forceTranscode: true, process: nativeScale(270, 480), processedWidth: 270, processedHeight: 480 }, audio: { discard: true } });
    await c.execute();
    // its packets into a new MP4, with captions beside them
    const src = new Input({ source: new BufferSource((first.target as BufferTarget).buffer!), formats: ALL_FORMATS });
    const v = (await src.getPrimaryVideoTrack())!;
    const out = new Output({ format: new MkvOutputFormat(), target: new BufferTarget() });
    const video = new EncodedVideoPacketSource("avc");
    out.addVideoTrack(video);
    const subs = new TextSubtitleSource("webvtt");
    out.addSubtitleTrack(subs, { languageCode: "eng" });
    await out.start();
    const config = await v.getDecoderConfig();
    let firstPacket = true;
    for await (const p of new EncodedPacketSink(v).packets()) {
      await video.add(p, firstPacket ? { decoderConfig: config! } : undefined);
      firstPacket = false;
    }
    await subs.add(wordsToWebVTT([{ text: "Two", from: 0, to: 15 }, { text: "seconds.", from: 15, to: 30 }, { text: "Done", from: 40, to: 55 }], 30));
    await out.finalize();
    saveTarget(out.target as BufferTarget, tmp("subs.mkv"));
    // the video reads back; the subtitle track is written as Matroska's S_TEXT/WEBVTT with its cues (mediabunny 1.61
    // reads no subtitle tracks back, so the file's bytes are checked)
    const bytes = new Uint8Array((out.target as BufferTarget).buffer!);
    const text = Array.from(bytes, (b) => String.fromCharCode(b)).join("");
    const back = new Input({ source: new BufferSource(bytes.buffer), formats: ALL_FORMATS });
    expect((await back.getTracks()).map((t) => t.type)).toContain("video");
    expect(text).toContain("S_TEXT/WEBVTT");
    expect(text).toContain("Two seconds.");
    expect(text).toContain("Done");
  }, 120000);
});

describe.skipIf(!engine)("codecs: what the hardware encodes (engine)", () => {
  test("H.264 is offered; AV1 and VP9 only where VideoToolbox lists a hardware encoder for them", () => {
    const hw = hardwareEncodes();
    expect(hw.hardware).toContain("avc1");
    expect(NativeVideoEncoder.supports("avc")).toBe(true);
    expect(NativeVideoEncoder.supports("av1")).toBe(false);
    expect(NativeVideoEncoder.supports("vp9")).toBe(false);
    expect(hw.av1).toBe(hw.hardware.includes("av01"));
    expect(hw.vp9).toBe(hw.hardware.includes("vp09"));
  });
});
