# gputrace

`raea-gputrace`: GPU telemetry for raea's flight recorder. raea has several GPU users that often share one device:
the quads compositor, kgpu's kernels, rdb's renderer, Bevy's render graph and video frames. This crate records every
uncaught wgpu error and lost device with the subsystem behind it (read from the label each user puts first on what it
creates), and each submission's latency, so slow submissions and stalls show up in the run's trace.

It is a small library with no binary. [`engine/`](../engine), [`kgpu/`](../kgpu) and [`rdb/`](../rdb) depend on it.
