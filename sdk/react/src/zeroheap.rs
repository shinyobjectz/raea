//! ZeroHeap Z1's counting allocator (docs/zeroheap.feature): the system allocator, with each thread's allocations
//! counted while RAEA_ZEROHEAP=1. A binary opts in with `#[global_allocator] static A: Counting = Counting;`
//! (raea-motion does; the exported player doesn't). Counts are per thread, so a span on one thread (the op apply,
//! the JS thread's flush) reads its own allocations and nothing another thread made meanwhile.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Counting;

static ON: AtomicBool = AtomicBool::new(false);

thread_local! {
    static COUNT: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
    /// this thread's counters as other threads can read them, registered at its first counted allocation
    static SLOT: Cell<Option<&'static [AtomicU64; 2]>> = const { Cell::new(None) };
    /// registering this thread (its registration allocates; those go uncounted per thread)
    static REG: Cell<bool> = const { Cell::new(false) };
}

/// Every thread that allocated while counting was on: its name and counters (leaked, a few per process)
static THREADS: std::sync::Mutex<Vec<(String, &'static [AtomicU64; 2])>> = std::sync::Mutex::new(Vec::new());
use std::sync::atomic::AtomicU64;

/// Every thread's allocations and bytes together, while counting is on
static ALL: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];

#[inline]
fn count(bytes: usize) {
    if ON.load(Ordering::Relaxed) {
        ALL[0].fetch_add(1, Ordering::Relaxed);
        ALL[1].fetch_add(bytes as u64, Ordering::Relaxed);
        // try_with: a thread being torn down has no counter left, and frees and allocations then go uncounted
        let _ = COUNT.try_with(|c| {
            let (n, b) = c.get();
            c.set((n + 1, b + bytes as u64));
        });
        sample();
        let _ = SLOT.try_with(|s| match s.get() {
            Some(a) => {
                a[0].fetch_add(1, Ordering::Relaxed);
                a[1].fetch_add(bytes as u64, Ordering::Relaxed);
            }
            None => {
                if REG.try_with(|r| r.replace(true)).unwrap_or(true) {
                    return;
                }
                let a: &'static [AtomicU64; 2] = Box::leak(Box::new([AtomicU64::new(0), AtomicU64::new(0)]));
                let name = std::thread::current().name().map(str::to_string).unwrap_or_else(|| format!("{:?}", std::thread::current().id()));
                if let Ok(mut t) = THREADS.lock() {
                    t.push((name, a));
                }
                s.set(Some(a));
                let _ = REG.try_with(|r| r.set(false));
            }
        });
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count(l.size());
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        count(l.size());
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(p, l, size) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}

/// Turn counting on when RAEA_ZEROHEAP=1 (once, at startup). Off, an allocation costs one relaxed load more.
pub fn init() {
    if std::env::var("RAEA_ZEROHEAP").is_ok_and(|v| v == "1") {
        ON.store(true, Ordering::Relaxed);
    }
}

/// This thread's allocations and bytes so far (both 0 while counting is off or the allocator isn't installed).
pub fn here() -> (u64, u64) {
    COUNT.try_with(|c| c.get()).unwrap_or((0, 0))
}

/// Every thread's allocations and bytes so far (0 while counting is off)
pub fn all() -> (u64, u64) {
    (ALL[0].load(Ordering::Relaxed), ALL[1].load(Ordering::Relaxed))
}

/// Each thread's name and allocations and bytes so far, while counting was on
pub fn threads() -> Vec<(String, u64, u64)> {
    THREADS.lock().map(|t| t.iter().map(|(n, a)| (n.clone(), a[0].load(Ordering::Relaxed), a[1].load(Ordering::Relaxed))).collect()).unwrap_or_default()
}

/// RAEA_ZEROHEAP_SITES=N: every Nth counted allocation's backtrace, summarized by its first frames outside the
/// allocator and std, tallied by that summary (slow while on: for attributing, not timing)
static PERIOD: AtomicU64 = AtomicU64::new(0);
static TICK: AtomicU64 = AtomicU64::new(0);
/// the sampled backtraces, captured but not resolved: symbols are looked up when sites() is read, never inside an
/// allocation (resolving there could wait on a lock the allocating code holds)
static SAMPLED: std::sync::Mutex<Vec<std::backtrace::Backtrace>> = std::sync::Mutex::new(Vec::new());

/// Turn sampling on (with init, when RAEA_ZEROHEAP_SITES is set).
pub fn init_sites() {
    if let Some(n) = std::env::var("RAEA_ZEROHEAP_SITES").ok().and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0) {
        PERIOD.store(n, Ordering::Relaxed);
    }
}

#[inline]
fn sample() {
    let p = PERIOD.load(Ordering::Relaxed);
    if p == 0 || TICK.fetch_add(1, Ordering::Relaxed) % p != 0 {
        return;
    }
    if REG.try_with(|r| r.replace(true)).unwrap_or(true) {
        return;
    }
    let bt = std::backtrace::Backtrace::force_capture();
    if let Ok(mut v) = SAMPLED.try_lock() {
        v.push(bt);
    }
    let _ = REG.try_with(|r| r.set(false));
}

/// The sampled sites, most first: (summary, samples). Resolves the backtraces (slow), with counting paused.
pub fn sites() -> Vec<(String, u64)> {
    let was = ON.swap(false, Ordering::Relaxed);
    let taken = SAMPLED.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
    let skip = ["std::", "core::", "alloc::", "zeroheap", "backtrace", "__rust", "rust_begin", "<alloc::", "hashbrown::raw", "<hashbrown"];
    let mut tally: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for bt in &taken {
        let text = format!("{bt}");
        let mut kept: Vec<&str> = Vec::new();
        for l in text.lines() {
            let l = l.trim();
            let Some((_, sym)) = l.split_once(": ") else { continue };
            if l.starts_with("at ") || skip.iter().any(|s| sym.contains(s)) {
                continue;
            }
            kept.push(sym);
            if kept.len() == 5 {
                break;
            }
        }
        *tally.entry(kept.join(" < ")).or_insert(0) += 1;
    }
    ON.store(was, Ordering::Relaxed);
    let mut v: Vec<(String, u64)> = tally.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}
