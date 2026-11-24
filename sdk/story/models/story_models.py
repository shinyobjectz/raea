"""The story models (docs/story.feature ST6b): an extractor, a generator and a critic, each a LoRA on one small base,
trained with MLX on this Mac and decoded under a JSON schema (llguidance), so every answer is data raea/story reads.
The generator's schema is the story's own (data.ts beatSchema): it can't name an event or a setup the story lacks.

  python story_models.py train   TASK [--rows ~/.vd/story/rows] [--iters N] [--base Qwen/Qwen3-0.6B]
  python story_models.py predict TASK --split test [--adapter DIR | --zero-shot] [--limit N]
  python story_models.py frontier TASK --split test --model anthropic/claude-sonnet-5.5 [--limit N]
  python story_models.py export  TASK --adapter DIR            (LoRA fused, then ONNX int4 for ort: DIR/onnx)
  python story_models.py onnx    TASK --split test --adapter DIR  (predictions from the ONNX model, under the schema)

Predictions go to ROWS/TASK/pred-<who>-<split>.jsonl; models/score.ts scores them with raea/story's exact checks.
"""

import argparse
import json
import math
import os
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

HOME = Path.home()

SYSTEM = {
    "extract": (
        "You read a short film or game script and write the story behind it as raea/story JSON: values (what's at "
        "stake, each {neg, pos}), facts (named facts), agents (want, need, lie, mask, role), world (events in the order "
        "they happen, each {who, does, because, sets, turns ...}; every event after the first names its cause or is "
        "routine; turns move values -3..3) and tellings.main (the beats in the script's order, each {shows: [event "
        "keys], feel, reveals, hides, asks, setup, pays}). Keys are short camelCase words. Answer with JSON only."
    ),
    "steps": (
        "You read a long film or game script a chunk at a time. You are given KNOWN, what earlier chunks found, in "
        "story lines (VALUE key: neg / pos, FACT key: text, AGENT key: want | need | lie | mask, EVENT key: who -- does "
        "| because | sets | turns), and the next SCRIPT chunk. Answer as raea/story JSON with only what's new in this "
        "chunk: values, facts, agents, world (events first met here, in the order they happen, each {who, does, because, "
        "sets, turns ...}; a cause can be a KNOWN event), and beats (this chunk's beats in order, each {shows: [event "
        "keys, KNOWN or new], feel, reveals, hides, asks, setup, pays}). Never repeat a KNOWN key. Keys are short "
        "camelCase words. Answer with JSON only."
    ),
    "generate": (
        "You are given a story in story lines: its values, facts, agents and the events it can still show; its STATE "
        "before the missing beat (value LEVELS, what the audience KNOWS, OPEN questions, SUSPENSE, UNPAID setups, "
        "UNSETTLED wants and needs, events AHEAD); the beats BEFORE it and AFTER it. Write the missing beat as JSON: "
        "{shows: [event keys], feel, reveals, hides, asks, setup, pays, onScreen, says}. It must follow from the beats "
        "before, lead into the beats after, turn a value or reveal something, and pay off what's still open. "
        "onScreen is what the viewer sees; says are the lines [{who, says}]. Answer with JSON only."
    ),
    "critic": (
        "You read a short film or game script, numbered by beat, and say where its story breaks: {breaks: [{beat, "
        "check, level}]}. beat is the beat's number, or null for the whole story. check is one of: turns (a beat "
        "changes nothing), and-then (a beat doesn't follow from anything seen), surprise (a big turn nothing set up), "
        "cause (an event with no cause), dead-end, setup (planted and never paid, or paid with no setup), belief (a "
        "character acts on what they can't know), rhythm, arc (a want or need never settled), claim (a claim with no "
        "proof shown), idea (the story ends where it began). level is error or warn. A story that holds has "
        "{breaks: []}. Answer with JSON only."
    ),
}


def rows_dir(a):
    return Path(os.path.expanduser(a.rows))


def read_jsonl(p):
    return [json.loads(l) for l in open(p) if l.strip()]


def canon(x, schema):
    """x with its keys in the schema's order: llguidance writes properties in that order, so training answers do too."""
    if schema is None:
        return x
    if "anyOf" in schema:
        for s in schema["anyOf"]:
            t = s.get("type")
            if (t == "array" and isinstance(x, list)) or (t == "object" and isinstance(x, dict)):
                return canon(x, s)
        return x
    if isinstance(x, dict):
        props = schema.get("properties", {})
        extra = schema.get("additionalProperties")
        out = {k: canon(x[k], props[k]) for k in props if k in x}
        for k, v in x.items():
            if k not in props and isinstance(extra, dict):
                out[k] = canon(v, extra)
        return out
    if isinstance(x, list):
        return [canon(v, schema.get("items")) for v in x]
    return x


def schema_for(task, row, schemas):
    s = row["schema"] if task == "generate" else schemas[task]
    s = json.loads(json.dumps(s))
    s["x-guidance"] = {"whitespace_flexible": False, "item_separator": ",", "key_separator": ":"}
    return s


def prompt_text(tok, task, user):
    msgs = [{"role": "system", "content": SYSTEM[task]}, {"role": "user", "content": user}]
    return tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True, enable_thinking=False)


class Rows:
    """A task's rows as (tokens, offset): the loss is on the answer alone."""

    def __init__(self, tok, task, rows, schemas, max_len):
        self.items = []
        for r in rows:
            p = prompt_text(tok, task, r["prompt"])
            ans = json.dumps(canon(json.loads(r["answer"]), schema_for(task, r, schemas)), separators=(",", ":"), ensure_ascii=False)
            pt = tok.encode(p)
            at = tok.encode(ans + "<|im_end|>\n")
            if len(pt) + len(at) <= max_len:
                self.items.append((pt + at, len(pt)))

    def __getitem__(self, i):
        return self.items[i]

    def __len__(self):
        return len(self.items)

    def itemlen(self, i):
        return len(self.items[i][0])


def train(a):
    import mlx.core as mx
    import mlx.optimizers as optim
    from mlx_lm import load
    from mlx_lm.tuner.trainer import TrainingArgs, train as mtrain
    from mlx_lm.tuner.utils import linear_to_lora_layers, print_trainable_parameters

    rd = rows_dir(a)
    schemas = json.load(open(rd / "schemas.json"))
    model, tok = load(a.base)
    tr = Rows(tok, a.task, read_jsonl(rd / a.task / "train.jsonl"), schemas, a.max_len)
    va = Rows(tok, a.task, read_jsonl(rd / a.task / "val.jsonl"), schemas, a.max_len)
    print(f"{a.task}: {len(tr)} train rows, {len(va)} val rows (max {a.max_len} tokens)", flush=True)
    mx.random.seed(0)
    model.freeze()
    linear_to_lora_layers(model, a.layers, {"rank": a.rank, "scale": 20.0, "dropout": 0.05})
    print_trainable_parameters(model)
    out = Path(a.adapter or (rd / a.task / f"adapter-{a.name}"))
    out.mkdir(parents=True, exist_ok=True)
    json.dump({"base": a.base, "rank": a.rank, "layers": a.layers, "iters": a.iters, "lr": a.lr, "rows": len(tr), "task": a.task}, open(out / "story_adapter.json", "w"))
    # mlx-lm's own adapter config, so mlx_lm.fuse and load(adapter_path=) read it
    json.dump({"fine_tune_type": "lora", "num_layers": a.layers, "lora_parameters": {"rank": a.rank, "scale": 20.0, "dropout": 0.05}, "model": a.base}, open(out / "adapter_config.json", "w"))
    steps = a.iters
    sched = optim.join_schedules([optim.linear_schedule(0, a.lr, max(1, steps // 20)), optim.cosine_decay(a.lr, steps)], [max(1, steps // 20)])
    args = TrainingArgs(batch_size=a.batch, iters=steps, val_batches=-1, steps_per_report=20, steps_per_eval=max(50, steps // 6),
                        steps_per_save=steps, adapter_file=str(out / "adapters.safetensors"), max_seq_length=a.max_len, grad_checkpoint=True)
    t = time.time()
    mtrain(model, optim.AdamW(learning_rate=sched, weight_decay=0.0), tr, va, args)
    print(f"trained {a.task} in {time.time() - t:.0f}s -> {out}", flush=True)


class Constrained:
    """Decoding under a JSON schema: llguidance's matcher masks every token the schema can't take next."""

    def __init__(self, tok, schema):
        import llguidance
        import llguidance.hf

        self.ll_tok = getattr(Constrained, "_ll", None)
        if self.ll_tok is None:
            Constrained._ll = self.ll_tok = llguidance.hf.from_tokenizer(tok._tokenizer)
        grammar = llguidance.LLMatcher.grammar_from_json_schema(schema)
        self.m = llguidance.LLMatcher(self.ll_tok, grammar)
        err = self.m.get_error()
        if err:
            raise ValueError(err)
        from llguidance.numpy import allocate_token_bitmask

        self.mask = allocate_token_bitmask(1, self.ll_tok.vocab_size)
        self.n = None

    def __call__(self, tokens, logits):
        from llguidance.mlx import apply_token_bitmask
        from llguidance.numpy import fill_next_token_bitmask

        toks = tokens.tolist() if hasattr(tokens, "tolist") else list(tokens)
        if self.n is None:
            self.n = len(toks)
        else:
            for t in toks[self.n:]:
                self.m.consume_token(t)
            self.n = len(toks)
        if self.m.is_stopped():
            return logits
        fill_next_token_bitmask(self.m, self.mask, 0)
        v = logits.shape[-1]
        out = apply_token_bitmask(logits[..., : self.ll_tok.vocab_size] if v > self.ll_tok.vocab_size else logits, self.mask)
        if v > self.ll_tok.vocab_size:
            import mlx.core as mx

            out = mx.concatenate([out, mx.full((1, v - self.ll_tok.vocab_size), -float("inf"), dtype=out.dtype)], axis=-1)
        return out


def zero_shot_user(task, row, schemas):
    """An untrained model gets the schema in words too: it has never seen the format."""
    s = schema_for(task, row, schemas)
    s.pop("x-guidance", None)
    return f"{row['prompt']}\n\nThe answer's JSON schema:\n{json.dumps(s, separators=(',', ':'))}"


def predict(a):
    from mlx_lm import load, stream_generate
    from mlx_lm.sample_utils import make_sampler

    rd = rows_dir(a)
    schemas = json.load(open(rd / "schemas.json"))
    rows = read_jsonl(rd / a.task / f"{a.split}.jsonl")[: a.limit or None]
    who = "zero" if a.zero_shot else Path(a.adapter).name
    model, tok = load(a.base, adapter_path=None if a.zero_shot else a.adapter)
    out = rd / a.task / f"pred-{who}-{a.split}.jsonl"
    done = {json.loads(l)["key"] for l in open(out)} if out.exists() else set()
    sampler = make_sampler(temp=0.0)
    t0, n = time.time(), 0
    with open(out, "a") as f:
        for r in rows:
            key = f"{r['i']}:{r.get('at', r.get('kind', ''))}"
            if key in done:
                continue
            user = zero_shot_user(a.task, r, schemas) if a.zero_shot else r["prompt"]
            p = prompt_text(tok, a.task, user)
            proc = Constrained(tok, schema_for(a.task, r, schemas))
            text, t = "", time.time()
            for resp in stream_generate(model, tok, p, max_tokens=a.max_new, sampler=sampler, logits_processors=[proc]):
                text += resp.text
                if proc.m.is_stopped():
                    break
            n += 1
            f.write(json.dumps({"key": key, "i": r["i"], "at": r.get("at"), "kind": r.get("kind"), "pred": text, "secs": round(time.time() - t, 2)}) + "\n")
            f.flush()
            if n % 10 == 0:
                print(f"{n}/{len(rows)} in {time.time() - t0:.0f}s", flush=True)
    print(f"predicted {n} -> {out}", flush=True)


def chain(a):
    """The windowed extractor end to end: each story's chunks in order, each step handed what the earlier steps found
    (as story lines, written by models/known.ts), then merged; written as extract predictions, scored by score.ts."""
    from mlx_lm import load, stream_generate
    from mlx_lm.sample_utils import make_sampler

    rd = rows_dir(a)
    schemas = json.load(open(rd / "schemas.json"))
    steps = read_jsonl(rd / "steps" / f"{a.split}.jsonl")
    stories = {}
    for r in steps:
        stories.setdefault(r["i"], []).append(r)
    ids = sorted(stories)[: a.limit or None]
    who = ("zero" if a.zero_shot else Path(a.adapter).name) + "-chain"
    model, tok = load(a.base, adapter_path=None if a.zero_shot else a.adapter)
    helper = subprocess.Popen(["bun", str(Path(__file__).parent / "known.ts")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

    def ask(q):
        helper.stdin.write(json.dumps(q) + "\n")
        helper.stdin.flush()
        return json.loads(helper.stdout.readline())

    out = rd / "extract" / f"pred-{who}-{a.split}.jsonl"
    done = {json.loads(l)["key"] for l in open(out)} if out.exists() else set()
    sampler = make_sampler(temp=0.0)
    t0 = time.time()
    with open(out, "a") as f:
        for n, i in enumerate(ids):
            key = f"{i}:script"
            if key in done:
                continue
            answers, t, bad = [], time.time(), 0
            for r in sorted(stories[i], key=lambda r: r["at"]):
                user = ask({"prompt": {"answers": answers, "script": r["script"]}})["prompt"]
                row = dict(r, prompt=user)
                if a.zero_shot:
                    user = zero_shot_user("steps", row, schemas)
                proc = Constrained(tok, schema_for("steps", row, schemas))
                text = ""
                for resp in stream_generate(model, tok, prompt_text(tok, "steps", user), max_tokens=a.max_new, sampler=sampler, logits_processors=[proc]):
                    text += resp.text
                    if proc.m.is_stopped():
                        break
                try:
                    answers.append(json.loads(text))
                except json.JSONDecodeError:
                    bad += 1
                    answers.append({})
            story = ask({"merge": answers})["story"]
            f.write(json.dumps({"key": key, "i": i, "kind": "script", "pred": json.dumps(story), "steps": len(answers), "unparsed": bad, "secs": round(time.time() - t, 2)}) + "\n")
            f.flush()
            if (n + 1) % 5 == 0:
                print(f"{n + 1}/{len(ids)} stories in {time.time() - t0:.0f}s", flush=True)
    helper.stdin.close()
    print(f"chained {len(ids)} -> {out}", flush=True)


def frontier(a):
    rd = rows_dir(a)
    schemas = json.load(open(rd / "schemas.json"))
    rows = read_jsonl(rd / a.task / f"{a.split}.jsonl")[: a.limit or None]
    key_ = re.search(r"^OPENROUTER_API_KEY=(.+)$", open(HOME / ".raea/env").read(), re.M).group(1).strip()
    who = "frontier-" + a.model.split("/")[-1]
    out = rd / a.task / f"pred-{who}-{a.split}.jsonl"
    done = {json.loads(l)["key"] for l in open(out)} if out.exists() else set()
    spent = 0.0
    from concurrent.futures import ThreadPoolExecutor

    def one(r):
        key = f"{r['i']}:{r.get('at', r.get('kind', ''))}"
        body = {"model": a.model, "temperature": 0, "max_tokens": 4000, "usage": {"include": True}, "response_format": {"type": "json_object"},
                "messages": [{"role": "system", "content": SYSTEM[a.task]}, {"role": "user", "content": zero_shot_user(a.task, r, schemas)}]}
        for attempt in range(3):
            try:
                req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=json.dumps(body).encode(),
                                             headers={"Authorization": f"Bearer {key_}", "Content-Type": "application/json", "User-Agent": "story-eval"})
                t = time.time()
                j = json.load(urllib.request.urlopen(req, timeout=180))
                return {"key": key, "i": r["i"], "at": r.get("at"), "kind": r.get("kind"), "pred": j["choices"][0]["message"]["content"] or "",
                        "secs": round(time.time() - t, 2), "usd": j.get("usage", {}).get("cost", 0)}
            except Exception as e:  # retried, then said
                err = str(e)
                time.sleep(2 * (attempt + 1))
        return {"key": key, "i": r["i"], "at": r.get("at"), "kind": r.get("kind"), "pred": "", "error": err}

    todo = [r for r in rows if f"{r['i']}:{r.get('at', r.get('kind', ''))}" not in done]
    with open(out, "a") as f, ThreadPoolExecutor(8) as ex:
        for res in ex.map(one, todo):
            spent += res.get("usd", 0) or 0
            f.write(json.dumps(res) + "\n")
            f.flush()
    print(f"frontier {a.model}: {len(todo)} rows, ${spent:.3f} -> {out}", flush=True)


def export(a):
    import shutil


    ad = Path(os.path.expanduser(a.adapter))
    fused, out = ad / "fused", ad / "onnx"
    py = sys.executable
    subprocess.run([py, "-m", "mlx_lm", "fuse", "--model", a.base, "--adapter-path", str(ad), "--save-path", str(fused), "--dequantize"], check=True, capture_output=True)
    subprocess.run([py, "-m", "onnxruntime_genai.models.builder", "-m", str(fused), "-o", str(out), "-p", "int4", "-e", "cpu"], check=True, capture_output=True)
    shutil.rmtree(fused)  # the fused weights are 1.2 GB and rebuilt from the adapter in seconds
    size = sum(f.stat().st_size for f in out.iterdir())
    print(f"exported {a.task} -> {out} ({size / 1e6:.0f} MB, int4, CPU)", flush=True)


def onnx_predict(a):
    import onnxruntime_genai as og
    from transformers import AutoTokenizer

    rd = rows_dir(a)
    schemas = json.load(open(rd / "schemas.json"))
    rows = read_jsonl(rd / a.task / f"{a.split}.jsonl")[: a.limit or None]
    d = Path(os.path.expanduser(a.adapter)) / "onnx"
    tok = AutoTokenizer.from_pretrained(str(d))
    model = og.Model(str(d))
    otok = og.Tokenizer(model)
    out = rd / a.task / f"pred-{Path(a.adapter).name}-onnx-{a.split}.jsonl"
    done = {json.loads(l)["key"] for l in open(out)} if out.exists() else set()
    with open(out, "a") as f:
        for r in rows:
            key = f"{r['i']}:{r.get('at', r.get('kind', ''))}"
            if key in done:
                continue
            p = tok.apply_chat_template([{"role": "system", "content": SYSTEM[a.task]}, {"role": "user", "content": r["prompt"]}], tokenize=False, add_generation_prompt=True, enable_thinking=False)
            ids = otok.encode(p)
            params = og.GeneratorParams(model)
            params.set_search_options(max_length=len(ids) + a.max_new, do_sample=False)
            params.set_guidance("json_schema", json.dumps(schema_for(a.task, r, schemas)))
            g = og.Generator(model, params)
            g.append_tokens(ids)
            toks, t = [], time.time()
            while not g.is_done():
                g.generate_next_token()
                toks.append(g.get_next_tokens()[0])
            f.write(json.dumps({"key": key, "i": r["i"], "at": r.get("at"), "kind": r.get("kind"), "pred": otok.decode(toks), "secs": round(time.time() - t, 2)}) + "\n")
            f.flush()
    print(f"onnx predictions -> {out}", flush=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["train", "predict", "chain", "frontier", "export", "onnx"])
    ap.add_argument("task", choices=["extract", "steps", "generate", "critic"])
    ap.add_argument("--rows", default="~/.vd/story/rows")
    ap.add_argument("--base", default="Qwen/Qwen3-0.6B")
    ap.add_argument("--name", default="q06")
    ap.add_argument("--adapter")
    ap.add_argument("--iters", type=int, default=600)
    ap.add_argument("--batch", type=int, default=4)
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--rank", type=int, default=16)
    ap.add_argument("--layers", type=int, default=28)
    ap.add_argument("--max-len", dest="max_len", type=int, default=4096)
    ap.add_argument("--max-new", dest="max_new", type=int, default=2500)
    ap.add_argument("--split", default="test")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--zero-shot", dest="zero_shot", action="store_true")
    ap.add_argument("--model", default="anthropic/claude-sonnet-5.5")
    a = ap.parse_args()
    {"train": train, "predict": predict, "chain": chain, "frontier": frontier, "export": export, "onnx": onnx_predict}[a.cmd](a)


if __name__ == "__main__":
    main()
