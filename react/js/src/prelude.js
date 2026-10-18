// The host side of bevy-react's bridge on our own V8 (no deno_core): __bevyHost's five calls, timers and microtasks.
(() => {
  const g = globalThis;
  g.console = { log: (...a) => __raea_log(0, a.map(String).join(" ")), info: (...a) => __raea_log(0, a.map(String).join(" ")),
    warn: (...a) => __raea_log(1, a.map(String).join(" ")), error: (...a) => __raea_log(2, a.map((x) => (x && x.stack) || String(x)).join(" ")),
    debug: () => {} };
  g.queueMicrotask = (fn) => { Promise.resolve().then(fn); };
  // timers: kept here, run by the host's loop through __raea_timers(now) (returns ms to the next one, or -1)
  let seq = 1; const timers = new Map();
  g.setTimeout = (fn, ms = 0, ...args) => { const id = seq++; timers.set(id, { at: __raea_now() + Math.max(0, ms), fn, args }); return id; };
  g.clearTimeout = (id) => { timers.delete(id); };
  g.setInterval = (fn, ms = 0) => { const id = seq++; const tick = () => { fn(); if (timers.has(id)) timers.set(id, { at: __raea_now() + ms, fn: tick, args: [] }); }; timers.set(id, { at: __raea_now() + ms, fn: tick, args: [] }); return id; };
  g.clearInterval = g.clearTimeout;
  g.__raea_timers = (now) => {
    for (const [id, t] of [...timers].sort((a, b) => a[1].at - b[1].at)) {
      if (t.at > now) break;
      timers.delete(id);
      try { t.fn(...t.args); } catch (e) { console.error(e); }
    }
    let next = -1;
    for (const t of timers.values()) next = next < 0 ? t.at - now : Math.min(next, t.at - now);
    return next;
  };
  g.performance = { now: () => __raea_now() };
  g.process = { env: { NODE_ENV: "production" } };
  g.__bevyHost = {
    op_flush: (json, devtools) => __raea_flush(json),
    op_emit: () => {},
    op_request: () => {},
    op_animate: () => {},
    // events from Bevy: queued until the bridge asks, or handed to the waiting ask (null = Bevy shut down)
    op_next_event: () => (inbox.length ? Promise.resolve(inbox.shift()) : new Promise((r) => { waiter = r; })),
  };
  const inbox = []; let waiter = null;
  g.__raea_deliver = (json) => {
    const m = json === null ? null : JSON.parse(json);
    if (waiter) { const w = waiter; waiter = null; w(m); } else inbox.push(m);
  };
})();
