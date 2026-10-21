//! Video frames as textures on the paint device (One renderer, phase 3): a decoder's NV12 CVPixelBuffer (IOSurface
//! backed) becomes two textures, luma as R8 and chroma as RG8, through CoreVideo's Metal texture cache. Nothing is
//! copied: the compositor samples the decoder's memory and converts to RGB as it composites. A page's `<img
//! data-raea-texture=ID>` (blitz-paint draw_texture) draws whichever frame is set for ID.

/// One video's current frame on the GPU.
pub struct Frame {
    /// NV12: luma then chroma; BGRA: the one picture (VideoToolbox's own conversion, as renders' stills had it)
    pub views: Vec<wgpu::TextureView>,
    pub nv12: bool,
    pub w: u32,
    pub h: u32,
    /// what keeps the decoder's memory alive while the GPU reads it
    _keep: Keep,
}

impl Frame {
    /// A texture already on the paint device (a live 3D scene's target): sampled as an opaque image, kept alive here.
    pub fn texture(view: wgpu::TextureView, w: u32, h: u32) -> Frame {
        Frame { views: vec![view.clone()], nv12: false, w, h, _keep: Keep(Box::new(view)) }
    }
}

struct Keep(#[allow(dead_code)] Box<dyn std::any::Any>);
// the CoreVideo objects kept are only held and dropped here, never used across threads
unsafe impl Send for Keep {}

#[cfg(target_vendor = "apple")]
pub use apple::Cache;

#[cfg(target_vendor = "apple")]
mod apple {
    use std::ptr::NonNull;

    use objc2_core_foundation::CFRetained;
    use objc2_core_video::{CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer};
    use objc2_metal::{MTLPixelFormat, MTLTextureType};

    pub struct Cache(CFRetained<CVMetalTextureCache>);
    unsafe impl Send for Cache {}

    impl Cache {
        pub fn new(device: &wgpu::Device) -> Result<Cache, String> {
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.ok_or("video: the device isn't Metal")?;
            let mtl = hal.raw_device().clone();
            let mut c: *mut CVMetalTextureCache = std::ptr::null_mut();
            let r = unsafe { CVMetalTextureCache::create(None, None, &mtl, None, NonNull::new(&mut c).unwrap()) };
            if r != 0 || c.is_null() {
                return Err(format!("CVMetalTextureCacheCreate: {r}"));
            }
            Ok(Cache(unsafe { CFRetained::from_raw(NonNull::new(c).unwrap()) }))
        }

        /// Let the cache drop textures nobody holds.
        pub fn flush(&self) {
            self.0.flush(0);
        }

        #[allow(clippy::too_many_arguments)]
        fn plane(&self, device: &wgpu::Device, pb: &CVPixelBuffer, plane: usize, w: u32, h: u32, mtl: MTLPixelFormat, format: wgpu::TextureFormat) -> Result<(wgpu::Texture, wgpu::TextureView, CFRetained<CVMetalTexture>), String> {
            let mut t: *mut CVMetalTexture = std::ptr::null_mut();
            let r = unsafe { CVMetalTextureCache::create_texture_from_image(None, &self.0, pb, None, mtl, w as usize, h as usize, plane, NonNull::new(&mut t).unwrap()) };
            if r != 0 || t.is_null() {
                return Err(format!("CVMetalTextureCacheCreateTextureFromImage (plane {plane}): {r}"));
            }
            let cv = unsafe { CFRetained::from_raw(NonNull::new(t).unwrap()) };
            let raw = CVMetalTextureGetTexture(&cv).ok_or("CVMetalTextureGetTexture: none")?;
            let extent = wgpu::hal::CopyExtent { width: w, height: h, depth: 1 };
            let hal_tex = unsafe { wgpu::hal::metal::Device::texture_from_raw(raw, format, MTLTextureType::Type2D, 1, 1, extent) };
            let desc = wgpu::TextureDescriptor {
                label: Some("video plane"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            let tex = unsafe { device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_tex, &desc) };
            let view = tex.create_view(&Default::default());
            Ok((tex, view, cv))
        }

        /// A decoded buffer as a frame, wrapped not copied: NV12 as two planes, BGRA as one.
        pub fn wrap(&self, device: &wgpu::Device, buffer: CFRetained<CVPixelBuffer>, w: u32, h: u32) -> Result<super::Frame, String> {
            let bgra = objc2_core_video::CVPixelBufferGetPixelFormatType(&buffer) == objc2_core_video::kCVPixelFormatType_32BGRA;
            if bgra {
                let (t, v, cv) = self.plane(device, &buffer, 0, w, h, MTLPixelFormat::BGRA8Unorm, wgpu::TextureFormat::Bgra8Unorm)?;
                return Ok(super::Frame { views: vec![v], nv12: false, w, h, _keep: super::Keep(Box::new((t, cv, buffer))) });
            }
            let (ty, yv, ycv) = self.plane(device, &buffer, 0, w, h, MTLPixelFormat::R8Unorm, wgpu::TextureFormat::R8Unorm)?;
            let (tu, uv, ucv) = self.plane(device, &buffer, 1, w.div_ceil(2), h.div_ceil(2), MTLPixelFormat::RG8Unorm, wgpu::TextureFormat::Rg8Unorm)?;
            Ok(super::Frame { views: vec![yv, uv], nv12: true, w, h, _keep: super::Keep(Box::new((ty, tu, ycv, ucv, buffer))) })
        }
    }
}
