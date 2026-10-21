"""Stable Audio Open Small on Core ML: one sound for a prompt and a seed, as a 44.1 kHz stereo WAV peak-normalised to
full scale. rdb's asset store runs it for make("sao", prompt, seed, {seconds}) and keeps the bytes under their hash
(sdk/react/docs/audio-stack.md, "Generated sources"); tools/sao/setup.sh installs it beside the models.
    run.py --prompt TEXT --wav OUT.wav [--seed 1] [--seconds 4] [--steps 8] [--sampler pingpong|euler]
           [--dit-units cpu|gpu|all] [--dir DIR]

The pipeline is the Core ML packages' (mlboydaisuke/Stable-Audio-Open-Small-CoreML), driven the way its Swift sample
app drives them: T5 -> number embedder -> DiT x steps -> VAE decoder, at a fixed 256 latent frames (11.9 s), the output
trimmed to the asked length. T5 is raea's own fp32 conversion (t5_to_coreml.py), because the published fp16 one
overflows (NaN on the GPU, inf on the CPU). Compute units are as published: T5, the number embedder and the decoder on
CPU+GPU, the DiT on the CPU alone (fp16 overflows in it). The sampler is pingpong at 8 steps, what the model was
distilled for, on the exact schedule stable-audio-tools uses.

This is ~/.cache/raea-sao/coreml_run.py (the 2026-10-15 spike) cut to what inference needs: numpy, coremltools and
Hugging Face's `tokenizers` reading t5-base's tokenizer.json from the install, so a run needs neither torch nor
stable-audio-tools nor the network. Same prompt and seed, same samples as the spike's runner (checked 2026-10-19)."""
import argparse, os, subprocess, sys

SR, LAT, AUDIO = 44100, 256, 524288
TOKENS = 64
# the 8-step schedule stable-audio-tools uses with the pingpong sampler (captured; the same at 44 and 256 frames)
PINGPONG8 = [1.0, 0.994376, 0.98448, 0.957912, 0.890903, 0.745547, 0.512497, 0.273885, 0.0]


def schedule(steps, np):
    """Their logSNR schedule, for any other step count or the Euler sampler."""
    t = 1 / (1 + np.exp(-6.0 + np.arange(steps + 1) / steps * 8.0))
    t[0], t[-1] = 1.0, 0.0
    return t.astype(np.float32)


def tokenize(tok_path, prompt, np):
    """t5-base's fast tokenizer as transformers runs it: </s> appended, cut to 64 tokens, padded with id 0."""
    from tokenizers import Tokenizer
    tok = Tokenizer.from_file(tok_path)
    tok.enable_truncation(TOKENS)
    tok.enable_padding(length=TOKENS, pad_id=0, pad_token="<pad>")
    e = tok.encode(prompt)
    return np.array([e.ids], np.int32), np.array([e.attention_mask], np.int32)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prompt", required=True)
    ap.add_argument("--wav", required=True)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--seconds", type=float, default=4.0)
    ap.add_argument("--steps", type=int, default=8)
    ap.add_argument("--sampler", default="pingpong", choices=["pingpong", "euler"])
    ap.add_argument("--dit-units", default="cpu", choices=["cpu", "gpu", "all"])
    ap.add_argument("--dir", default=os.path.dirname(os.path.abspath(__file__)))
    a = ap.parse_args()

    import numpy as np
    import coremltools as ct

    units = {"cpu": ct.ComputeUnit.CPU_ONLY, "gpu": ct.ComputeUnit.CPU_AND_GPU, "all": ct.ComputeUnit.ALL}
    ref = f"{a.dir}/coreml-ref"
    load = lambda n, u: ct.models.MLModel(f"{ref}/StableAudio{n}.mlpackage", compute_units=units[u])
    t5 = ct.models.MLModel(f"{a.dir}/coreml/T5EncoderFP32.mlpackage", compute_units=units["gpu"])
    num, dit, dec = load("NumberEmbedder", "gpu"), load("DiT", a.dit_units), load("VAEDecoder", "gpu")

    secs = min(a.seconds, AUDIO / SR)
    ids, mask = tokenize(f"{a.dir}/t5-tokenizer/tokenizer.json", a.prompt, np)
    emb = t5.predict({"input_ids": ids, "attention_mask": mask})["text_embeddings"].astype(np.float32)
    emb = emb * mask[..., None]  # padding zeroed, as the PyTorch conditioner does
    sec = num.predict({"normalized_seconds": np.array([min(secs, 256) / 256], np.float32)})["seconds_embedding"].astype(np.float32)
    cross = np.concatenate([emb, sec[:, None, :]], axis=1)

    rng = np.random.default_rng(a.seed)
    x = rng.standard_normal((1, 64, LAT)).astype(np.float32)
    ts = np.array(PINGPONG8, np.float32) if a.sampler == "pingpong" and a.steps == 8 else schedule(a.steps, np)
    for s in range(a.steps):
        v = dit.predict({"latent": x, "timestep": np.array([ts[s]], np.float32), "cross_attn_cond": cross, "global_embed": sec})["velocity"]
        v = v.astype(np.float32)
        if a.sampler == "pingpong":  # predict the clean latent, then re-noise to the next level
            den = x - ts[s] * v
            x = (1 - ts[s + 1]) * den + ts[s + 1] * rng.standard_normal(x.shape).astype(np.float32)
        else:
            x = x + (ts[s + 1] - ts[s]) * v
    audio = dec.predict({"latent": x})["audio"][0].astype(np.float32)[:, : int(secs * SR)]
    audio = np.nan_to_num(audio)
    audio = audio / max(np.abs(audio).max(), 1e-6)
    pcm = (audio.T * 32767).astype("<i2").tobytes()
    # ffmpeg writes the WAV, as the spike's runner did, so the bytes match it too
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "s16le", "-ar", str(SR), "-ac", "2", "-i", "-", a.wav], input=pcm, check=True)


if __name__ == "__main__":
    sys.exit(main())
