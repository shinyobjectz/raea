"""TokenSynth's smaller runtime, made from the fp32 weights setup.sh fetched (run at the end of setup.sh; idempotent: a
file already made is kept, so a second run does nothing). Everything goes beside weights/, outside the repo:
  weights-fp16/  the transformer, its unconditional twin, DAC's quantizer + decoder and CLAP's text tower (RoBERTa and
                 its projection, without the audio tower or the optimizer state that make up most of the 2.35 GB CLAP
                 file), as fp16 state dicts; plus the RoBERTa tokenizer and config, so run.py needs no Hugging Face cache.
  coreml/        Core ML models, compiled (.mlmodelc): DAC's quantizer + decoder (codes in, audio out, fixed at 448
                 frames, 5.2 s, TokenSynth's longest output: run.py pads the codes and trims the audio), computing in
                 fp32 (in fp16 its noise floor shows in the quiet tail: CLAP cosine to fp32 down to 0.82 on the same
                 tokens); and one step of the transformer, fp16, with its KV cache as state.
run.py --precision picks among them (fp32 reads weights/ only).
    convert.py [--dir ~/.cache/raea-tokensynth] [--only fp16|coreml]"""
import argparse, os, sys, time, warnings
from pathlib import Path

warnings.filterwarnings("ignore")
ap = argparse.ArgumentParser()
ap.add_argument("--dir", default=os.environ.get("RAEA_TOKENSYNTH", str(Path.home() / ".cache/raea-tokensynth")))
ap.add_argument("--only", choices=["fp16", "coreml"])
args = ap.parse_args()

import torch

D = Path(args.dir)
W, F, C = D / "weights", D / "weights-fp16", D / "coreml"
FRAMES = 448  # TokenSynth writes at most 450 steps less its 8-step delay pattern: 442 frames; 448 leaves room


def save(obj, path):
    """Write through a .part file, so an interrupted run leaves nothing that looks finished."""
    tmp = path.with_name(path.name + ".part")
    torch.save(obj, tmp)
    os.replace(tmp, path)


def half(sd, skip=()):
    return {k: (v.half() if v.is_floating_point() else v).contiguous() for k, v in sd.items() if not any(s in k for s in skip)}


def fp16():
    F.mkdir(parents=True, exist_ok=True)
    for name in ("token_synth_aug", "token_synth_unconditional"):
        out = F / f"{name}.pt"
        if not out.exists():
            # the causal masks are rebuilt at load (12 MB of bools each), the KV caches are runtime state
            save(half(torch.load(W / f"{name}.pt", map_location="cpu", weights_only=True), skip=(".attn.mask", "_cache")), out)
            print(f"convert: {out.name}", file=sys.stderr)
    out = F / "dac_decoder.pt"
    if not out.exists():
        ck = torch.load(W / "dac_weights_44khz_8kbps_0.0.1.pt", map_location="cpu", weights_only=False)
        sd = {k: v for k, v in ck["state_dict"].items() if k.startswith(("quantizer.", "decoder."))}
        save({"kwargs": ck["metadata"]["kwargs"], "state_dict": half(sd)}, out)
        print(f"convert: {out.name}", file=sys.stderr)
    out = F / "clap_text.pt"
    if not out.exists():
        ck = torch.load(W / "clap_music_audioset_epoch_15_esc_90.14.pt", map_location="cpu", weights_only=False)["state_dict"]
        sd = {k[len("module."):]: v for k, v in ck.items() if k.startswith(("module.text_branch.", "module.text_projection."))}
        save(half(sd), out)
        print(f"convert: {out.name}", file=sys.stderr)
    tok = F / "roberta"
    if not (tok / "config.json").exists():
        from transformers import RobertaConfig, RobertaTokenizer
        RobertaTokenizer.from_pretrained("roberta-base").save_pretrained(tok)
        RobertaConfig.from_pretrained("roberta-base").save_pretrained(tok)
        print("convert: roberta tokenizer + config", file=sys.stderr)


def _run():
    import importlib.util
    spec = importlib.util.spec_from_file_location("ts_run", Path(__file__).resolve().parent / "run.py")
    run = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(run)
    return run


def _keep(ml, name):
    """The compiled .mlmodelc, which loads without compiling again (0.3 s instead of 1-6 s for the .mlpackage)."""
    import shutil
    import coremltools as ct
    C.mkdir(parents=True, exist_ok=True)
    tmp = C / f"{name}.part.mlpackage"
    shutil.rmtree(tmp, ignore_errors=True)
    ml.save(str(tmp))
    loaded = ct.models.MLModel(str(tmp), compute_units=ct.ComputeUnit.CPU_AND_GPU)  # its compiled copy lives while it does
    ctmp = C / f"{name}.part.mlmodelc"
    shutil.rmtree(ctmp, ignore_errors=True)
    shutil.copytree(loaded.get_compiled_model_path(), ctmp)
    del loaded
    os.replace(ctmp, C / f"{name}.mlmodelc")
    shutil.rmtree(tmp)  # run.py loads the compiled copy only


def coreml_dac():
    if (C / "dac_decoder.mlmodelc").exists():
        return
    import coremltools as ct
    dec = _run().load_dac(F, torch.float32).eval()

    class Codes2Audio(torch.nn.Module):
        def __init__(self, d):
            super().__init__()
            self.d = d

        def forward(self, codes):
            return self.d.decoder(self.d.quantizer.from_codes(codes)[0])

    m = Codes2Audio(dec).eval()
    # weight norm folded in, so the trace holds plain convolutions; Snake without its TorchScript (whose shape
    # indexing Core ML can't convert), the same arithmetic
    from dac.nn.layers import Snake1d
    for mod in m.modules():
        if hasattr(mod, "weight_g"):
            torch.nn.utils.remove_weight_norm(mod)
        if isinstance(mod, Snake1d):
            mod.forward = (lambda a: lambda x: x + (a + 1e-9).reciprocal() * torch.sin(a * x).pow(2))(mod.alpha)
    ex = torch.randint(0, 1024, (1, 9, FRAMES), dtype=torch.int32)
    t0 = time.time()
    with torch.no_grad():
        traced = torch.jit.trace(m, ex.long())
    ml = ct.convert(traced, inputs=[ct.TensorType(name="codes", shape=ex.shape, dtype=ex.numpy().dtype)], outputs=[ct.TensorType(name="audio")],
                    compute_precision=ct.precision.FLOAT32, minimum_deployment_target=ct.target.macOS14, convert_to="mlprogram")
    _keep(ml, "dac_decoder")
    print(f"convert: dac_decoder.mlpackage ({time.time() - t0:.0f} s)", file=sys.stderr)


def coreml_step():
    """The transformer's 12 blocks, its final norm and head, one position per call, the KV caches held as Core ML
    state (macOS 15). The step writes this position's keys and values where `onehot` is 1 and attends where `mask` is
    0 (-1e4 elsewhere): static shapes, which the GPU runs faster than a dynamic slice. The embeddings (token, CLAP,
    position) stay in torch: run.py adds them up and hands the step the sum."""
    if (C / "token_synth_step.mlmodelc").exists():
        return
    import numpy as np
    import coremltools as ct
    from tokensynth import TokenSynth
    ts = _run().load_transformer(TokenSynth, F / "token_synth_aug.pt", torch.float32)
    hp = ts.hparams
    L, H, N = hp.num_blocks, hp.num_heads, hp.max_len
    HD = hp.embed_dim // H

    class Step(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.blocks, self.ln, self.fc = ts.blocks, ts.ln, ts.fc
            for i in range(L):
                self.register_buffer(f"k{i}", torch.zeros(1, H, N, HD))
                self.register_buffer(f"v{i}", torch.zeros(1, H, N, HD))

        def forward(self, x, onehot, mask):
            for i, b in enumerate(self.blocks):
                a, h = b.attn, b.ln1(x)
                q = a.query(h).view(1, 1, H, HD).transpose(1, 2)
                k = a.key(h).view(1, 1, H, HD).transpose(1, 2)
                v = a.value(h).view(1, 1, H, HD).transpose(1, 2)
                kc, vc = getattr(self, f"k{i}"), getattr(self, f"v{i}")
                kc[:, :, :, :] = kc * (1 - onehot) + k * onehot
                vc[:, :, :, :] = vc * (1 - onehot) + v * onehot
                y = ((q @ kc.transpose(2, 3)) * HD ** -0.5 + mask).softmax(-1) @ vc
                x = x + a.proj(y.transpose(1, 2).reshape(1, 1, H * HD))
                x = x + b.ff(b.ln2(x))
            return self.fc(self.ln(x))

    ex = (torch.zeros(1, 1, hp.embed_dim), torch.zeros(1, 1, N, 1), torch.zeros(1, 1, 1, N))
    t0 = time.time()
    with torch.no_grad():
        traced = torch.jit.trace(Step().eval(), ex)
    states = [ct.StateType(wrapped_type=ct.TensorType(shape=(1, H, N, HD), dtype=np.float16), name=f"{kv}{i}") for i in range(L) for kv in "kv"]
    ml = ct.convert(traced, inputs=[ct.TensorType(name=n, shape=t.shape, dtype=np.float16) for n, t in zip(("x", "onehot", "mask"), ex)],
                    outputs=[ct.TensorType(name="logits", dtype=np.float16)], states=states, compute_precision=ct.precision.FLOAT16,
                    minimum_deployment_target=ct.target.macOS15, convert_to="mlprogram")
    _keep(ml, "token_synth_step")
    print(f"convert: token_synth_step.mlpackage ({time.time() - t0:.0f} s)", file=sys.stderr)


if args.only in (None, "fp16"):
    fp16()
if args.only in (None, "coreml"):
    try:
        coreml_dac()
        coreml_step()
    except Exception as e:  # Core ML needs macOS 15 for the stateful step; the other precisions don't need it
        if args.only:
            raise
        print(f"convert: no Core ML models ({type(e).__name__}: {e}); run.py --precision coreml won't run", file=sys.stderr)
