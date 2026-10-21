// mediabunny on the engine's coders, run where it lands (the iPhone probe runs it in the app's Documents folder with
// clip.mp4 beside it): a probe, a trim, a proxy, 16 kHz audio and a remux, each written beside the clip for checking
// against ffmpeg on the Mac (sdk/motion/tools/media-vs-ffmpeg.sh's jobs, on the device's VideoToolbox).
import { BufferTarget, Conversion, Mp4OutputFormat, Output, WavOutputFormat } from "mediabunny";
import { fileInput, nativeScale, saveTarget, transcode, useNativeMedia } from "@raea/motion/codecs";
import { print } from "@raea/motion/native";

const now = () => (globalThis as any).__raea_now() as number;

async function convert(name: string, format: Mp4OutputFormat | WavOutputFormat, opts: Record<string, unknown>) {
  const t = now();
  const out = new Output({ format, target: new BufferTarget() });
  const c = await Conversion.init({ input: fileInput("clip.mp4"), output: out, ...opts });
  await c.execute();
  saveTarget(out.target as BufferTarget, name);
  print(`${name} in ${(now() - t).toFixed(0)} ms`);
}

export default async function main() {
  useNativeMedia();
  const input = fileInput("clip.mp4");
  const v = (await input.getPrimaryVideoTrack())!;
  const a = await input.getPrimaryAudioTrack();
  const stats = await v.computePacketStats();
  print("PROBE " + JSON.stringify({ duration: await input.computeDuration(), width: v.displayWidth, height: v.displayHeight, codec: v.codec, frames: stats.packetCount, audio: a ? { codec: a.codec, rate: a.sampleRate, channels: a.numberOfChannels } : null }));
  await convert("trim.mp4", new Mp4OutputFormat(), { trim: { start: 1, end: 3 }, video: { codec: "avc", forceTranscode: true } });
  const h = 720, w = Math.round((v.displayWidth * h) / v.displayHeight / 2) * 2;
  await convert("proxy.mp4", new Mp4OutputFormat(), { video: { codec: "avc", forceTranscode: true, process: nativeScale(w, h), processedWidth: w, processedHeight: h }, audio: { discard: true } });
  await convert("a16.wav", new WavOutputFormat(), { video: { discard: true }, audio: { sampleRate: 16000, numberOfChannels: 1, codec: "pcm-s16" } });
  await convert("remux.mp4", new Mp4OutputFormat(), {});
  const t = now();
  const r = transcode("clip.mp4", "native-proxy.mp4", { height: 720 });
  print(`native-proxy.mp4 in ${(now() - t).toFixed(0)} ms (${r.frames} frames)`);
}
