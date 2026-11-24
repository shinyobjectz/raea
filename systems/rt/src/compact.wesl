// raea GI's world cache, its upkeep each frame (cache.rs; docs/realism.feature LT3): every cell ages a frame, one no
// ray has read for LIFE frames empties, and the live ones are listed for the update (cache_update.wesl) to walk. The
// list is a prefix sum over the cells' liveness in two levels: within each block of 1024 cells, then over the blocks'
// totals (CELLS is 1024 blocks of 1024). From vendor/bevy_solari's world_cache_compact.wesl, made raea's.

import raea_sys_rt::cache::{checksums, life, radiance, deltas, a, b, live, count, dispatch, EMPTY};

var<workgroup> w1: array<u32, 1024u>;
var<workgroup> w2: array<u32, 1024u>;

// a cell a frame older; one that reaches 0 empties
@compute @workgroup_size(1024, 1, 1)
fn decay(@builtin(global_invocation_id) id: vec3<u32>) {
    var l = atomicLoad(&life[id.x]);
    if (l > 0u) {
        l -= 1u;
        atomicStore(&life[id.x], l);
        if (l == 0u) {
            atomicStore(&checksums[id.x], EMPTY);
            radiance[id.x] = vec4(0.0);
            deltas[id.x] = 0.0;
        }
    }
}

// An exclusive prefix sum of 1024 values in w1 (Hillis-Steele, ping-ponged through w2), left in w1 for thread t
// to read: ten doublings.
fn scan(t: u32) -> u32 {
    workgroupBarrier();
    for (var stride = 1u; stride < 1024u; stride = stride << 1u) {
        let here = w1[t];
        // (select reads both sides: the index is clamped, the value dropped)
        let back = select(0u, w1[select(0u, t - stride, t >= stride)], t >= stride);
        workgroupBarrier();
        w2[t] = here + back;
        workgroupBarrier();
        w1[t] = w2[t];
        workgroupBarrier();
    }
    return w1[t];
}

// within each block: how many live cells come before this one
@compute @workgroup_size(1024, 1, 1)
fn scan_cells(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) t: u32) {
    w1[t] = select(0u, u32(atomicLoad(&life[max(id.x, 1u) - 1u]) != 0u), t > 0u);
    a[id.x] = scan(t);
}

// over the blocks: how many live cells come before each block (one workgroup, a thread a block)
@compute @workgroup_size(1024, 1, 1)
fn scan_blocks(@builtin(local_invocation_index) t: u32) {
    let last = max(t * 1024u, 1u) - 1u;
    w1[t] = select(0u, a[last] + u32(atomicLoad(&life[last]) != 0u), t > 0u);
    b[t] = scan(t);
}

// each live cell into its place in the list; the last thread writes the count and the update's dispatch
@compute @workgroup_size(1024, 1, 1)
fn list(@builtin(global_invocation_id) id: vec3<u32>, @builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) t: u32) {
    let at = a[id.x] + b[wg.x];
    let on = atomicLoad(&life[id.x]) != 0u;
    if (on) {
        live[at] = id.x;
    }
    if (t == 1023u && wg.x == 1023u) {
        let n = at + u32(on);
        count = n;
        dispatch = vec3((n + 63u) / 64u, 1u, 1u);
    }
}
