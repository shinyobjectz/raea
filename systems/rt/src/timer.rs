//! A traced pass's GPU time, read back without waiting on the GPU: its timestamps resolved into a buffer that's mapped
//! once the frame's work is done, and read on a later frame. raea GI's probes and the far sun's traces use it.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use bevy::render::renderer::{RenderDevice, RenderQueue};

/// A pass's timestamps and what rides along with them (`P`: what was timed) while they're on their way back.
pub(crate) struct Timer<P> {
    pub(crate) set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    pub(crate) read: wgpu::Buffer,
    stamps: u32,
    period: f64,
    pub(crate) pending: Option<(Arc<AtomicBool>, P)>,
}

impl<P> Timer<P> {
    /// `stamps` timestamps (two a pass), read back into `read` bytes: the stamps first, then anything the caller copies
    /// after them.
    pub(crate) fn new(device: &RenderDevice, queue: &RenderQueue, label: &str, stamps: u32, read: u64) -> Self {
        let d = device.wgpu_device();
        let set = d.create_query_set(&wgpu::QuerySetDescriptor { label: Some(label), ty: wgpu::QueryType::Timestamp, count: stamps });
        let buffer = |what: &str, size: u64, usage: wgpu::BufferUsages| {
            d.create_buffer(&wgpu::BufferDescriptor { label: Some(&format!("{label} {what}")), size, usage, mapped_at_creation: false })
        };
        let resolve = buffer("resolve", 8 * stamps as u64, wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC);
        let read = buffer("read", read, wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        Timer { set, resolve, read, stamps, period: queue.get_timestamp_period() as f64, pending: None }
    }

    /// A pass's start and end, into stamps `first` and `first + 1`.
    pub(crate) fn writes(&self, first: u32) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites { query_set: &self.set, beginning_of_pass_write_index: Some(first), end_of_pass_write_index: Some(first + 1) }
    }

    /// Every stamp into the read-back, after the timed passes.
    pub(crate) fn resolve_into(&self, enc: &mut wgpu::CommandEncoder) {
        enc.resolve_query_set(&self.set, 0..self.stamps, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, 8 * self.stamps as u64);
    }

    /// After the submit: the read-back mapped when the GPU is done with it, `what` kept beside it.
    pub(crate) fn send(&mut self, what: P) {
        let done = Arc::new(AtomicBool::new(false));
        let d = done.clone();
        self.read.slice(..).map_async(wgpu::MapMode::Read, move |r| d.store(r.is_ok(), Ordering::Release));
        self.pending = Some((done, what));
    }

    /// The read-back once it's mapped: `read` gets the milliseconds between two stamps and the bytes, then the buffer is
    /// let go for the next timed frame. None while it's still on its way.
    pub(crate) fn take<R>(&mut self, read: impl FnOnce(&dyn Fn(usize, usize) -> f64, &[u8]) -> R) -> Option<(R, P)> {
        if !self.pending.as_ref()?.0.load(Ordering::Acquire) {
            return None;
        }
        let r = {
            let view = self.read.slice(..).get_mapped_range().ok()?;
            let ts = |i: usize| u64::from_le_bytes(view[8 * i..8 * i + 8].try_into().unwrap());
            let ms = |a: usize, b: usize| ts(b).saturating_sub(ts(a)) as f64 * self.period / 1e6;
            read(&ms, &view)
        };
        self.read.unmap();
        self.pending.take().map(|(_, what)| (r, what))
    }
}
