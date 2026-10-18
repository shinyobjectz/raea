//! Optical flow through Apple Vision (VNGenerateOpticalFlowRequest, on the Neural Engine and GPU; no model shipped),
//! reduced per frame pair to what editing rules ask about motion (the roadmap Track L, the FLOW slot):
//!
//!   grid   mean flow magnitude in a 3x3 grid, as a share of the frame's width per frame pair (where things move)
//!   dx dy  the mean flow (a pan or a scroll), same unit; dy > 0 is downward
//!   zoom   the mean radial flow away from the centre over distance from it (> 0 zooming in, < 0 out)
//!   mag    the largest grid cell's magnitude (a panel zooming in a still frame shows here, not in the mean)

use objc2::AnyThread;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{VNGenerateOpticalFlowRequest, VNGenerateOpticalFlowRequestComputationAccuracy, VNImageRequestHandler, VNRequest};

use crate::coreml::Bgra;

#[derive(Clone, Debug, Default)]
pub struct Flow {
    pub grid: [f32; 9],
    pub dx: f32,
    pub dy: f32,
    pub zoom: f32,
    pub mag: f32,
}

/// The flow from `a` to `b` (same size).
pub fn flow(a: &Bgra, b: &Bgra) -> Result<Flow, String> {
    let (src, dst) = (a.pixel_buffer()?, b.pixel_buffer()?);
    let req = unsafe { VNGenerateOpticalFlowRequest::initWithTargetedCVPixelBuffer_options(VNGenerateOpticalFlowRequest::alloc(), &dst, &NSDictionary::new()) };
    unsafe { req.setComputationAccuracy(VNGenerateOpticalFlowRequestComputationAccuracy::Medium) };
    let handler = unsafe { VNImageRequestHandler::initWithCVPixelBuffer_options(VNImageRequestHandler::alloc(), &src, &NSDictionary::new()) };
    let r: &VNRequest = &req;
    handler.performRequests_error(&NSArray::from_slice(&[r])).map_err(|e| format!("optical flow: {e:?}"))?;
    let obs = unsafe { req.results() }.and_then(|rs| rs.firstObject()).ok_or("optical flow: no result")?;
    let pb = unsafe { obs.pixelBuffer() };
    unsafe { CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly) };
    let (w, h, row) = (CVPixelBufferGetWidth(&pb), CVPixelBufferGetHeight(&pb), CVPixelBufferGetBytesPerRow(&pb));
    let base = CVPixelBufferGetBaseAddress(&pb) as *const u8;
    let mut f = Flow::default();
    let (mut cells, mut counts) = ([0f64; 9], [0usize; 9]);
    let (mut sx, mut sy, mut sz, mut nz) = (0f64, 0f64, 0f64, 0usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    for y in 0..h {
        let line = unsafe { std::slice::from_raw_parts(base.add(y * row) as *const f32, w * 2) };
        for x in 0..w {
            let (dx, dy) = (line[2 * x] as f64, line[2 * x + 1] as f64);
            let c = (y * 3 / h) * 3 + x * 3 / w;
            cells[c] += (dx * dx + dy * dy).sqrt();
            counts[c] += 1;
            sx += dx;
            sy += dy;
            let (rx, ry) = (x as f64 - cx, y as f64 - cy);
            let d = (rx * rx + ry * ry).sqrt();
            if d > 0.1 * w as f64 {
                sz += (dx * rx + dy * ry) / (d * d);
                nz += 1;
            }
        }
    }
    unsafe { CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly) };
    let n = (w * h).max(1) as f64;
    for i in 0..9 {
        f.grid[i] = (cells[i] / counts[i].max(1) as f64 / w as f64) as f32;
    }
    f.dx = (sx / n / w as f64) as f32;
    f.dy = (sy / n / w as f64) as f32;
    f.zoom = (sz / nz.max(1) as f64) as f32;
    f.mag = f.grid.iter().cloned().fold(0.0, f32::max);
    Ok(f)
}
