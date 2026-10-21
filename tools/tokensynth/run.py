"""TokenSynth: one note of an instrument described in words, rendered locally as a 44.1 kHz float WAV, normalised to
-1 dBFS peak. rdb's asset store runs it for make("tokensynth", timbre, seed, {midi, velocity, seconds, precision})
and keeps the bytes under their hash (sdk/react/docs/audio-stack.md, "Generated sources"). TokenSynth reads notes that end
before 5.0 s; `seconds` is the note's held length, and the release rings on up to 1.5 s after it.
    run.py --timbre TEXT --midi N [--velocity 100] [--seconds 2] [--seed 1234] --wav OUT.wav [--weights DIR]
           [--precision fp32|fp16|bf16|int8|coreml]
--precision picks the runtime; all but fp32 read what convert.py made beside DIR (weights-fp16/, coreml/), skip
laion_clap's audio tower and CLAP's 2.35 GB file, and stop generating once the frames cover the note and its release
(TokenSynth otherwise runs its 450-step cap; the audio is the same, CLAP cosine 1.000 on the same tokens):
  fp32    the original: TokenSynth's own code and the fp32 checkpoints, on the CPU (the bytes made before 10-18)
  fp16    (default) fp16 weights; the transformers compute in fp16 on the CPU, CLAP's text tower and DAC's decoder in fp32
  bf16    as fp16, computing in bfloat16
  int8    the transformers' Linear layers dynamically quantised to int8 (qnnpack): slower and further from fp32
  coreml  the transformer's step (fp16) and DAC's decoder (fp32) in Core ML on the GPU, the rest as fp16; macOS 15
Measured 10-18 on the M5 Pro (one note per process, load average 13): fp32 11.9 s a note, fp16 4.3 s (1.5 s note) and
5.6 s (3 s note), coreml 5.8 and 6.9 s (Core ML's load eats its faster decode). Fed fp32's own tokens, fp16's
next-token distributions differ from fp32's by 1e-5 nats (bf16 8e-4, int8 0.1). No precision gives fp32's bytes:
sampling diverges within the first frame (fp16-rounded weights alone do it), so each precision is its own sample and
part of what decides the bytes."""
import argparse, os, sys, tempfile, time
from pathlib import Path

PRECISIONS = ("fp32", "fp16", "bf16", "int8", "coreml")
DEFAULT = "fp16"
STOP_EARLY = True
SR = 44100
NAMES = {"clap": "clap_music_audioset_epoch_15_esc_90.14.pt", "dac": "dac_weights_44khz_8kbps_0.0.1.pt", "token_synth_aug": "token_synth_aug.pt", "token_synth_unconditional": "token_synth_unconditional.pt"}
# the weights from the install only: never the network, never ~/Library/Caches
os.environ.setdefault("HF_HUB_OFFLINE", "1")

import numpy as np, torch


class _no_init:
    """Build modules without initialising their weights: they are overwritten by the checkpoint right after."""
    KINDS = (torch.nn.Linear, torch.nn.Embedding, torch.nn.Conv1d, torch.nn.ConvTranspose1d, torch.nn.LayerNorm)

    def __enter__(self):
        self.saved = [(k, k.reset_parameters) for k in self.KINDS]
        for k in self.KINDS:
            k.reset_parameters = lambda self: None
        self.hf = None
        if "transformers" in sys.modules:  # the caller building a Hugging Face model has imported it
            from transformers.modeling_utils import no_init_weights
            self.hf = no_init_weights(_enable=True)
            self.hf.__enter__()

    def __exit__(self, *exc):
        for k, f in self.saved:
            k.reset_parameters = f
        if self.hf:
            self.hf.__exit__(*exc)


def _bare_package(name):
    """Import `name`'s modules without running its __init__ (tokensynth's imports laion_clap and DAC: 2-4 s)."""
    import importlib.util, types
    if name not in sys.modules:
        pkg = types.ModuleType(name)
        pkg.__path__ = list(importlib.util.find_spec(name).submodule_search_locations)
        sys.modules[name] = pkg


def _sd(path):
    return torch.load(path, map_location="cpu", weights_only=True, mmap=True)


def _load(m, sd, rebuilt=()):
    """Load and check: every parameter must come from the file (they were left uninitialised), buffers only those
    rebuilt after."""
    missing = [k for k in m.load_state_dict(sd, strict=False, assign=True).missing_keys if not k.endswith(rebuilt)]
    if missing:
        raise SystemExit(f"tokensynth: the converted weights lack {missing[:3]}: rerun tools/tokensynth/convert.py")


def load_transformer(cls, path, dtype):
    with _no_init():
        m = cls(device=torch.device("cpu"))
    _load(m, _sd(path), rebuilt=(".attn.mask", ".attn.k_cache", ".attn.v_cache"))
    hp = m.hparams
    for b in m.blocks:
        # the checkpoints' masks are lower-triangular (True: may attend), not the triu the class builds
        b.attn.mask = torch.tril(torch.ones(hp.max_len, hp.max_len).bool()).unsqueeze(0).unsqueeze(0)
    m = m.to(dtype).eval()
    for b in m.blocks:
        b.attn.k_cache.zero_()
        b.attn.v_cache.zero_()
    return m


class Dac(torch.nn.Module):
    """DAC's quantizer (codes to latents) and decoder, without the encoder."""

    def __init__(self, kw):
        super().__init__()
        from dac.model.dac import Decoder
        from dac.nn.quantize import ResidualVectorQuantize
        latent = kw["encoder_dim"] * 2 ** len(kw["encoder_rates"])
        self.quantizer = ResidualVectorQuantize(input_dim=latent, n_codebooks=kw["n_codebooks"], codebook_size=kw["codebook_size"], codebook_dim=kw["codebook_dim"])
        self.decoder = Decoder(latent, kw["decoder_dim"], kw["decoder_rates"])

    def forward(self, codes):
        return self.decoder(self.quantizer.from_codes(codes)[0])[0, 0]


def load_dac(F, dtype=torch.float32):
    ck = _sd(F / "dac_decoder.pt")
    with _no_init():
        m = Dac(ck["kwargs"])
    _load(m, ck["state_dict"])
    return m.to(dtype).eval()


class ClapText(torch.nn.Module):
    """CLAP's text tower alone (RoBERTa's pooled output through the projection, L2-normalised), as laion_clap computes it."""

    def __init__(self, F):
        super().__init__()
        from transformers import RobertaConfig, RobertaModel, RobertaTokenizer
        self.tok = RobertaTokenizer.from_pretrained(F / "roberta")
        with _no_init():
            self.text_branch = RobertaModel(RobertaConfig.from_pretrained(F / "roberta"))
            self.text_projection = torch.nn.Sequential(torch.nn.Linear(768, 512), torch.nn.ReLU(), torch.nn.Linear(512, 512))
        _load(self, _sd(F / "clap_text.pt"))
        self.float().eval()

    def encode_text(self, text):
        t = self.tok([text], padding="max_length", truncation=True, max_length=77, return_tensors="pt")
        x = self.text_branch(input_ids=t["input_ids"], attention_mask=t["attention_mask"])["pooler_output"]
        return torch.nn.functional.normalize(self.text_projection(x), dim=-1)


def _wrap(m, dtype, device):
    """The model computes in `dtype` on `device`; what it hands back is fp32 logits on the CPU, so the sampling (and the
    CPU generator the seed drives) is the same as fp32's."""
    fwd = m.forward

    def forward(x, *a, **k):
        a = [t.to(device=device, dtype=dtype) if torch.is_tensor(t) and t.is_floating_point() else t for t in a]
        return fwd(x.to(device), *a, **k).float().cpu()

    m.forward = forward
    m.device = torch.device("cpu")  # synthesize() builds its token buffers here


def reset(synth):
    """Both models as a fresh process has them. The first-note guidance can reach the unconditional model's KV cache
    without resetting it (when the first step is already a note), so a second note in one process would otherwise
    read the first one's cache."""
    for m in (synth, synth.unconditional_model):
        m.current_length = 0
        for b in m.blocks:
            b.attn.current_length = 0
            b.attn.k_cache.zero_()
            b.attn.v_cache.zero_()


def generate(synth, emb, midi_tokens, steps, top_p=0.6, guidance_scale=1.6):
    """TokenSynth.synthesize (the same steps, the same draws from the generator) without its progress bar, stopping
    after `steps` audio steps (its own cap is 450)."""
    from tokensynth import utils as ts_utils
    hp = synth.hparams
    synth.eval()
    reset(synth)
    midi_len = midi_tokens.shape[1]
    tokens = torch.zeros((1, hp.max_len, 10)).long()
    tokens[0, :midi_len, 0] = midi_tokens[0]
    synth.count = 0
    logit = synth.forward(tokens[:, :midi_len + 1], emb, use_cache=False)[:, -1, :]
    i = midi_len + 1
    for i in range(midi_len + 1, min(midi_len + 451, midi_len + 1 + steps, hp.max_len - 1)):
        if guidance_scale:
            is_silence = logit[:, hp.midi_vocab_size:hp.midi_vocab_size + 1024].argmax().item() == 569
            if not is_silence and synth.count < 9:
                synth.count += 1
                logit = synth.apply_first_note_guidance(logit, guidance_scale, midi_len, i, tokens)
        next_token = ts_utils.sample(logit, top_p=top_p, midi_vocab_size=hp.midi_vocab_size, audio_vocab_size=hp.audio_vocab_size)
        if (next_token == 0).all():
            break
        tokens[0, i, :] = next_token
        logit = synth.forward(tokens[:, i:i + 1], use_cache=True)[:, -1, :]
    end_idx = i if (next_token == 0).all() else i + 1
    return ts_utils.post_process_audio_tokens(tokens[:, midi_len + 1:end_idx, 1:])


class CoreMLSteps:
    """TokenSynth.forward with the blocks in Core ML (convert.py's token_synth_step), one position per call: torch
    adds up the token, CLAP and position embeddings, Core ML holds the KV caches."""

    def __init__(self, ts, path, cu):
        import coremltools as ct
        self.ts, self.m = ts, ct.models.CompiledMLModel(str(path), compute_units=cu)
        n = ts.hparams.max_len
        self.onehot, self.mask = np.zeros((1, 1, n, 1), np.float16), np.zeros((1, 1, 1, n), np.float16)

    def __call__(self, x, clap_embedding=None, use_cache=False):
        ts = self.ts
        e = ts.midi_tok_embed(x[:, :, 0])
        for i in range(9):
            e = e + ts.audio_tok_embed[i](x[:, :, i + 1])
        if clap_embedding is not None:
            e = torch.cat((ts.clap_projection(clap_embedding.float()).unsqueeze(1), e), dim=1)
        if not use_cache:
            self.state, self.n = self.m.make_state(), 0
            self.mask[:] = -1e4
        for j in range(e.shape[1]):
            self.onehot[:] = 0
            self.onehot[0, 0, self.n, 0] = 1
            self.mask[0, 0, 0, self.n] = 0
            v = (e[:, j:j + 1] + ts.pos_embed[:, self.n:self.n + 1]).numpy().astype(np.float16)
            out = self.m.predict({"x": v, "onehot": self.onehot, "mask": self.mask}, self.state)["logits"]
            self.n += 1
        ts.current_length = self.n
        return torch.from_numpy(out.astype(np.float32)).reshape(1, 1, -1)


class Runtime:
    def __init__(self, precision, weights, device="cpu"):
        self.precision, self.W = precision, Path(weights)
        self.F, self.C = self.W.parent / "weights-fp16", self.W.parent / "coreml"
        dev = torch.device(device)
        if precision != "fp32":
            _bare_package("tokensynth")
        from tokensynth import utils as ts_utils
        ts_utils.download_model = lambda name: self.W / NAMES[name]
        from tokensynth.model import TokenSynth, TokenSynthUnconditional
        with torch.no_grad():
            if precision == "fp32":
                from tokensynth import CLAP
                import dac
                self.synth = TokenSynth.from_pretrained(path=str(self.W / NAMES["token_synth_aug"]), device=dev)
                self.synth.load_unconditional_model()
                self.clap = CLAP(device=dev)
                self.dac = dac.DAC.load(self.W / NAMES["dac"]).eval()
                return
            missing = [p for p in ("token_synth_aug.pt", "token_synth_unconditional.pt", "dac_decoder.pt", "clap_text.pt", "roberta") if not (self.F / p).exists()]
            if precision == "coreml":
                missing += [f"../coreml/{p}" for p in ("dac_decoder.mlmodelc", "token_synth_step.mlmodelc") if not (self.C / p).exists()]
            if missing:
                raise SystemExit(f"tokensynth: {precision} needs {', '.join(missing)} in {self.F} (tools/tokensynth/setup.sh runs convert.py)")
            dtype = {"fp16": torch.float16, "bf16": torch.bfloat16, "coreml": torch.float16}.get(precision, torch.float32)
            synth = load_transformer(TokenSynth, self.F / "token_synth_aug.pt", dtype)
            uncond = load_transformer(TokenSynthUnconditional, self.F / "token_synth_unconditional.pt", dtype)
            if precision == "int8":
                torch.backends.quantized.engine = "qnnpack"
                q = lambda m: torch.ao.quantization.quantize_dynamic(m, {torch.nn.Linear}, dtype=torch.qint8, inplace=True)
                synth, uncond = q(synth), q(uncond)
            if dev.type != "cpu":
                synth, uncond = synth.to(dev), uncond.to(dev)
            _wrap(uncond, dtype, dev)
            if precision == "coreml":
                import coremltools as ct
                cu = ct.ComputeUnit[os.environ.get("RAEA_TOKENSYNTH_CU", "CPU_AND_GPU")]
                synth = synth.float()
                synth.forward = CoreMLSteps(synth, self.C / "token_synth_step.mlmodelc", cu)
                synth.blocks = torch.nn.Sequential()  # the weights live in Core ML; nothing for reset() to clear
            else:
                _wrap(synth, dtype, dev)
            synth.unconditional_model = uncond
            self.synth = synth
            self.clap = ClapText(self.F)
            if precision == "coreml":
                self.dac = ct.models.CompiledMLModel(str(self.C / "dac_decoder.mlmodelc"), compute_units=cu)
            else:
                self.dac = load_dac(self.F)

    def text(self, timbre):
        with torch.no_grad():
            return self.clap.encode_text(timbre)

    def tokens(self, emb, midi, velocity, held, seed):
        import pretty_midi
        from tokensynth import utils as ts_utils
        with torch.no_grad(), tempfile.TemporaryDirectory() as tmp:
            mid = os.path.join(tmp, "note.mid")
            pm = pretty_midi.PrettyMIDI(initial_tempo=120)
            inst = pretty_midi.Instrument(program=0)
            inst.notes.append(pretty_midi.Note(velocity=max(1, min(127, velocity)), pitch=midi, start=0.0, end=held))
            pm.instruments.append(inst)
            pm.write(mid)
            torch.manual_seed(seed)
            # TokenSynth runs to its 450-step cap (its end token hardly ever comes) and the note is cut at held + 1.5 s:
            # stop once the frames cover that, plus 8 frames of the delay pattern and 8 for the decoder's context.
            # fp32 keeps the full run, so its bytes are the ones made before the other precisions were.
            steps = 450 if self.precision == "fp32" or not STOP_EARLY else int(np.ceil((held + 1.5) * SR / 512)) + 16
            return generate(self.synth, emb, ts_utils.get_tokens_from_midi(mid), steps=steps)

    def decode(self, toks):
        codes = toks.permute(0, 2, 1)
        with torch.no_grad():
            if self.precision == "fp32":
                return self.dac.decode(self.dac.quantizer.from_codes(codes)[0])[0, 0].float().numpy()
            if self.precision == "coreml":
                n = codes.shape[2]
                frames = 448
                pad = np.zeros((1, 9, frames), dtype=np.int32)
                pad[:, :, :min(n, frames)] = codes[:, :, :frames].numpy()
                audio = self.dac.predict({"codes": pad})["audio"].reshape(-1)
                return audio[: n * 512].astype(np.float32)
            return self.dac(codes).float().numpy()

    def render(self, timbre, midi, velocity=100, seconds=2.0, seed=1234):
        held = max(0.05, min(seconds, 3.4))
        torch.manual_seed(seed)
        toks = self.tokens(self.text(timbre), midi, velocity, held, seed)
        return finish(self.decode(toks), held), toks


def finish(audio, held):
    end = min(len(audio), int((held + 1.5) * SR))
    audio = audio[:end].copy()
    # a 5 ms fade at the end, normalised to -1 dBFS peak (TokenSynth's output runs hot: peaks of 1.8 in the spike)
    fade = min(len(audio), int(0.005 * SR))
    if fade:
        audio[-fade:] *= np.linspace(1.0, 0.0, fade, dtype=np.float32)
    peak = float(np.abs(audio).max()) or 1.0
    return (audio * (10 ** (-1 / 20) / peak)).astype(np.float32), peak


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--timbre", required=True)
    ap.add_argument("--midi", type=int, required=True)
    ap.add_argument("--velocity", type=int, default=100)
    ap.add_argument("--seconds", type=float, default=2.0)
    ap.add_argument("--seed", type=int, default=1234)
    ap.add_argument("--wav", required=True)
    ap.add_argument("--weights", default=str(Path(__file__).resolve().parent / "weights"))
    ap.add_argument("--precision", choices=PRECISIONS, default=DEFAULT)
    args = ap.parse_args()
    import soundfile as sf
    rt = Runtime(args.precision, args.weights)
    (audio, peak), _ = rt.render(args.timbre, args.midi, args.velocity, args.seconds, args.seed)
    sf.write(args.wav, np.stack([audio, audio], axis=1), SR, subtype="FLOAT")
    print(f"tokensynth: midi {args.midi}, {len(audio) / SR:.2f} s, peak was {peak:.2f}, {args.precision}", file=sys.stderr)


if __name__ == "__main__":
    main()
