"""t5-base's encoder -> Core ML at fp32, for run.py: input_ids [1,64] int32, attention_mask [1,64] int32 ->
text_embeddings [1,64,768]. The fp16 encoder the Core ML packages publish overflows (NaN on the GPU, inf on the CPU,
garbage on the ANE), so SAO's T5 is this one. setup.sh runs it once, in a throwaway environment with torch,
transformers 5.19 and coremltools 9.0 (the versions the 2026-10-15 conversion used), then checks the result against
PyTorch on a few prompts: the cosine to PyTorch's embeddings must be above 0.999 on the CPU and the GPU.
    t5_to_coreml.py --out T5EncoderFP32.mlpackage [--revision SHA] [--tokenizer tokenizer.json]"""
import argparse, sys
import numpy as np, torch, coremltools as ct
from transformers import T5EncoderModel
from coremltools.converters.mil.frontend.torch.ops import _get_inputs
from coremltools.converters.mil.frontend.torch.torch_op_registry import register_torch_op
from coremltools.converters.mil import Builder as mb

ap = argparse.ArgumentParser()
ap.add_argument("--out", required=True)
ap.add_argument("--revision", default="a9723ea7f1b39c1eae772870f3b547bf6ef7e6c1")
ap.add_argument("--tokenizer", help="t5-base's tokenizer.json (else transformers fetches the tokenizer)")
a = ap.parse_args()

PROMPTS = [
    "Fingers typing quickly on a computer keyboard, crisp mechanical key clicks, quiet office room.",
    "A single fast cinematic whoosh swipe transition sound effect, clean, no music.",
    "A bright playful pop notification sound effect, short, clean.",
]


@register_torch_op
def new_ones(context, node):  # x.new_ones(size): ones of that size (fp32; T5 only uses it for the mask)
    inputs = _get_inputs(context, node)
    size = inputs[1]
    if not isinstance(size, (list, tuple)) and size.can_be_folded_to_const():
        dims = tuple(int(d) for d in np.asarray(size.val).ravel())  # () -> a scalar one
        context.add(mb.const(val=np.ones(dims, np.float32) if dims else np.float32(1.0), name=node.name))
        return
    shape = mb.concat(values=[mb.reshape(x=v, shape=[1]) for v in size], axis=0) if isinstance(size, (list, tuple)) else size
    context.add(mb.fill(shape=mb.cast(x=shape, dtype="int32"), value=1.0, name=node.name))


def tokenize(prompt):
    if a.tokenizer:
        from tokenizers import Tokenizer
        tok = Tokenizer.from_file(a.tokenizer)
        tok.enable_truncation(64)
        tok.enable_padding(length=64, pad_id=0, pad_token="<pad>")
        e = tok.encode(prompt)
        return np.array([e.ids], np.int32), np.array([e.attention_mask], np.int32)
    from transformers import AutoTokenizer
    t = AutoTokenizer.from_pretrained("google-t5/t5-base", revision=a.revision)(prompt, max_length=64, padding="max_length", truncation=True, return_tensors="np")
    return t["input_ids"].astype(np.int32), t["attention_mask"].astype(np.int32)


class Enc(torch.nn.Module):
    """T5's encoder with the mask built here, as an additive bias (relative-position bias + -1e9 on padding keys), so
    none of transformers' newer mask utilities end up in the graph."""

    def __init__(s, m):
        super().__init__()
        s.e = m.encoder

    def forward(s, input_ids, attention_mask):
        e = s.e
        h = e.embed_tokens(input_ids)
        bias = e.block[0].layer[0].SelfAttention.compute_bias(64, 64)  # [1, heads, 64, 64]
        bias = bias + (1.0 - attention_mask.float())[:, None, None, :] * -1e9
        for blk in e.block:
            h = blk(h, attention_mask=None, position_bias=bias)[0]
        return e.final_layer_norm(h)


pt = T5EncoderModel.from_pretrained("google-t5/t5-base", revision=a.revision, attn_implementation="eager").eval()
ids, mask = tokenize(PROMPTS[0])
with torch.no_grad():
    tr = torch.jit.trace(Enc(pt), (torch.from_numpy(ids), torch.from_numpy(mask)), check_trace=False)
ml = ct.convert(tr, inputs=[ct.TensorType(name="input_ids", shape=(1, 64), dtype=np.int32), ct.TensorType(name="attention_mask", shape=(1, 64), dtype=np.int32)],
                outputs=[ct.TensorType(name="text_embeddings")], compute_precision=ct.precision.FLOAT32,
                minimum_deployment_target=ct.target.iOS17, convert_to="mlprogram")
ml.save(a.out)
worst = 1.0
for u in ("CPU_ONLY", "CPU_AND_GPU"):
    m = ct.models.MLModel(a.out, compute_units=getattr(ct.ComputeUnit, u))
    for prompt in PROMPTS:
        ids, mask = tokenize(prompt)
        e = m.predict({"input_ids": ids, "attention_mask": mask})["text_embeddings"]
        with torch.no_grad():
            p = pt(input_ids=torch.from_numpy(ids).long(), attention_mask=torch.from_numpy(mask).long()).last_hidden_state.numpy()
        n = int(mask.sum())
        x, y = e[0, :n].ravel(), p[0, :n].ravel()
        worst = min(worst, float(x @ y / np.linalg.norm(x) / np.linalg.norm(y)) if np.isfinite(e).all() else 0.0)
print(f"T5 fp32: cosine to PyTorch at worst {worst:.5f}", file=sys.stderr)
sys.exit(0 if worst > 0.999 else 1)
