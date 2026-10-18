# kgpu

`raea-kgpu`: K on the GPU. A kernel is a few lines of a small K dialect; it compiles to one fused WGSL compute shader
on wgpu, the same GPU stack Bevy uses. The same text also runs on the CPU through an interpreter, and the two agree
bit for bit: everything is 32-bit integers with wrapping arithmetic and fixed loop orders, so there is nothing for a
GPU to reorder. `src/lib.rs` documents the dialect.

- `kernels/*.k`: the kernels raea uses, for compositing, NV12 conversion, frame comparison and per-frame columns.
  `kernels/gen.py` generates some of them.
- `src/bin/`: test and benchmark binaries that check each kernel against the interpreter.

[`rdb/`](../rdb) runs these kernels in its render path.

Status: in use.
