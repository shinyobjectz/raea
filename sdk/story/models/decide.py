"""The story decision model (docs/story.md, "Story as decisions"): System 1 for raea/story.

A small base (Qwen3-0.6B by default) reads a beat's window and every typed question about it in ONE prefill pass, with
no text generated. A small head scores each option of each question from the hidden state at that option's last
token, and a softmax over a question's options gives its probabilities. Trained as Clef is: the base frozen, LoRA on
its layers, the head; label-smoothed cross-entropy plus a Brier term so the probabilities are calibrated. The answers
are exact (models/decide-rows.ts, from the checks, systems and lenses), so this is consolidation: the slow, exact
reading (System 2) turned into a one-pass instinct, measured against it and against Clef (models/clef.ts).

  python decide.py train   [--rows ~/.vd/story/rows/decide] [--iters 3000] [--name q06]
  python decide.py predict [--split test] [--limit 300] [--name q06]       -> pred-decide-NAME-SPLIT.jsonl
Predictions take the same evenly spread windows clef.ts does, so decide-score.ts compares them on the same ones.
"""
import argparse
import json
import math
import random
import time
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
from mlx.utils import tree_flatten

SYSTEM = (
    "You read a story window and answer typed questions about one beat of it. Each question lists its options; "
    "the most fitting option is the answer."
)


def rows_dir(a):
    return Path(a.rows).expanduser()


def read_jsonl(p):
    return [json.loads(l) for l in open(p) if l.strip()]


def options(q):
    if q["type"] == "noul":
        return [("no", ""), ("yes", "")]
    if q["type"] == "choice":
        return list(q["criteria"].items())
    return [(str(i), lvl) for i, lvl in enumerate(q["criteria"])]


def truth(q, v):
    return ("yes" if v else "no") if q["type"] == "noul" else str(v)


def encode(tok, row, max_state_chars):
    """Token ids for the window and its questions, and per question its option names and their last-token positions."""
    enc = lambda s: tok.encode(s, add_special_tokens=False)
    state = row["state"]
    if len(state) > max_state_chars:  # keep the end (the beats and LOOK AT) and as much of the head as fits
        state = state[: max_state_chars // 3] + "\n...\n" + state[-(2 * max_state_chars) // 3 :]
    ids = enc(f"<|im_start|>system\n{SYSTEM}<|im_end|>\n<|im_start|>user\nSTATE\n{state}\nQUESTIONS\n")
    qs = []
    for qid, q in row["questions"].items():
        ids += enc(f"\n[{qid}] {q['instructions']}\n")
        names, pos = [], []
        for name, desc in options(q):
            ids += enc(f"- {name}{': ' + desc if desc and desc != name else ''}\n")
            names.append(name)
            pos.append(len(ids) - 1)
        qs.append((qid, names, pos))
    return ids, qs


class Head(nn.Module):
    def __init__(self, dims, hidden=256):
        super().__init__()
        self.l1 = nn.Linear(dims, hidden)
        self.l2 = nn.Linear(hidden, 1)

    def __call__(self, h):
        return self.l2(nn.gelu(self.l1(h))).squeeze(-1)


class Decider(nn.Module):
    def __init__(self, lm, dims):
        super().__init__()
        self.lm = lm
        self.head = Head(dims)

    def __call__(self, ids, pos):
        h = self.lm.model(ids[None])[0]  # hidden states after the final norm
        return self.head(h[pos])


def build(a, train=True):
    from mlx_lm import load
    from mlx_lm.tuner.utils import linear_to_lora_layers

    lm, tok = load(a.base)
    lm.freeze()
    linear_to_lora_layers(lm, a.layers, {"rank": a.rank, "scale": 20.0, "dropout": 0.05 if train else 0.0})
    dims = lm.model.embed_tokens.weight.shape[1]
    dec = Decider(lm, dims)
    return dec, tok


def loss_fn(dec, ids, pos, groups, targets, smooth=0.05):
    logits = dec(ids, pos)
    total = mx.array(0.0)
    for (s, e), t in zip(groups, targets):
        lg = logits[s:e]
        n = e - s
        lp = lg - mx.logsumexp(lg)
        onehot = mx.zeros((n,)).at[t].add(1.0)
        soft = onehot * (1 - smooth) + smooth / n
        p = mx.exp(lp)
        total = total + (-(soft * lp).sum()) + ((p - onehot) ** 2).sum()
    return total / len(groups)


def prep(tok, row, max_state_chars):
    ids, qs = encode(tok, row, max_state_chars)
    pos, groups, targets, names = [], [], [], []
    for qid, opts, p in qs:
        t = truth(row["questions"][qid], row["answers"][qid])
        if t not in opts:
            continue
        groups.append((len(pos), len(pos) + len(p)))
        targets.append(opts.index(t))
        pos += p
        names.append((qid, opts))
    return mx.array(ids), mx.array(pos), groups, targets, names


def train(a):
    from mlx_lm.tuner.trainer import grad_checkpoint

    rd = rows_dir(a)
    rows = read_jsonl(rd / "train.jsonl")
    allval = read_jsonl(rd / "val.jsonl")
    val = allval[:: max(1, len(allval) // a.val)][: a.val]
    dec, tok = build(a)
    grad_checkpoint(dec.lm.model.layers[0])
    n_train = sum(v.size for _, v in tree_flatten(dec.trainable_parameters()))
    print(f"decide: {len(rows)} train windows, {len(val)} val; trainable {n_train / 1e6:.2f}M", flush=True)
    sched = optim.join_schedules([optim.linear_schedule(1e-6, a.lr, 100), optim.cosine_decay(a.lr, a.iters, a.lr * 0.05)], [100])
    opt = optim.AdamW(learning_rate=sched, weight_decay=0.01)
    vg = nn.value_and_grad(dec, loss_fn)
    rng = random.Random(0)
    out = rd / f"decide-{a.name}"
    out.mkdir(exist_ok=True)

    def val_loss():
        dec.eval()
        ls = [loss_fn(dec, *prep(tok, r, a.max_state)[:4]).item() for r in val]
        dec.train()
        return sum(ls) / len(ls)

    print(f"     0  val {val_loss():.3f}", flush=True)
    t0, acc, grads = time.time(), [], None
    for it in range(1, a.iters + 1):
        r = rows[rng.randrange(len(rows))]
        ids, pos, groups, targets, _ = prep(tok, r, a.max_state)
        if not groups:
            continue
        l, g = vg(dec, ids, pos, groups, targets)
        grads = g if grads is None else tree_map_add(grads, g)
        acc.append(l.item())
        if it % a.accum == 0:
            grads = tree_scale(grads, 1 / a.accum)
            opt.update(dec, grads)
            mx.eval(dec.parameters(), opt.state)
            grads = None
        if it % 50 == 0:
            print(f"{it:6d}  {sum(acc) / len(acc):.3f}  {time.time() - t0:.0f}s", flush=True)
            acc = []
        if it % 500 == 0 or it == a.iters:
            print(f"{it:6d}  val {val_loss():.3f}", flush=True)
            mx.save_safetensors(str(out / "weights.safetensors"), dict(tree_flatten(dec.trainable_parameters())))
    json.dump(vars(a), open(out / "config.json", "w"), indent=1)
    print(f"trained -> {out}", flush=True)


def tree_map_add(a, b):
    from mlx.utils import tree_map
    return tree_map(lambda x, y: x + y, a, b)


def tree_scale(a, s):
    from mlx.utils import tree_map
    return tree_map(lambda x: x * s, a)


def predict(a):
    rd = rows_dir(a)
    allrows = read_jsonl(rd / f"{a.split}.jsonl")
    stride = max(1, len(allrows) // a.limit) if a.limit else 1
    rows = [r for j, r in enumerate(allrows) if j % stride == 0][: a.limit or None]
    dec, tok = build(a, train=False)
    dec.load_weights(str(rd / f"decide-{a.name}" / "weights.safetensors"), strict=False)
    dec.eval()
    out = rd / f"pred-decide-{a.name}-{a.split}.jsonl"
    with open(out, "w") as f:
        for n, r in enumerate(rows):
            t = time.time()
            ids, qs = encode(tok, r, a.max_state)
            pos = [p for _, _, ps in qs for p in ps]
            logits = dec(mx.array(ids), mx.array(pos))
            mx.eval(logits)
            probs, i = {}, 0
            for qid, names, ps in qs:
                lg = logits[i : i + len(ps)]
                p = mx.softmax(lg).tolist()
                probs[qid] = dict(zip(names, p))
                i += len(ps)
            key = f"{r['i']}:{r['kind']}:{r['at']}:{r['as']}"
            f.write(json.dumps({"key": key, "probs": probs, "secs": round(time.time() - t, 3), "tokens": len(ids)}) + "\n")
            if (n + 1) % 50 == 0:
                print(f"{n + 1}/{len(rows)}", flush=True)
    print(f"predicted {len(rows)} -> {out}", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["train", "predict"])
    ap.add_argument("--rows", default="~/.vd/story/rows/decide")
    ap.add_argument("--base", default="Qwen/Qwen3-0.6B")
    ap.add_argument("--name", default="q06")
    ap.add_argument("--iters", type=int, default=3000)
    ap.add_argument("--accum", type=int, default=4)
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--rank", type=int, default=16)
    ap.add_argument("--layers", type=int, default=28)
    ap.add_argument("--max-state", dest="max_state", type=int, default=9000)
    ap.add_argument("--val", type=int, default=60)
    ap.add_argument("--split", default="test")
    ap.add_argument("--limit", type=int, default=300)
    a = ap.parse_args()
    {"train": train, "predict": predict}[a.cmd](a)


if __name__ == "__main__":
    main()
