//! Core ML from Rust (the roadmap A2): the same .mlpackage files the apps run, compiled once into the founder's caches,
//! fed BGRA pixel buffers, read back as floats through each multi-array's strides (Core ML pads rows). Apple only;
//! other platforms run the ONNX exports through ort (later).

use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::AnyThread;
use objc2_core_foundation::CFRetained;
use objc2_core_ml::{
    MLComputeUnits, MLDictionaryFeatureProvider, MLFeatureProvider, MLFeatureValue, MLModel, MLModelConfiguration, MLMultiArray,
    MLMultiArrayDataType,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSURL};

/// Which processors a model may use.
#[derive(Clone, Copy, Debug)]
pub enum Units {
    All,
    CpuAndGpu,
    CpuOnly,
    /// the Neural Engine with the CPU, never the GPU (on an M5 Pro with macOS 27.0.1 the GPU computes SigLIP 2 wrong:
    /// cosine 0.855 to fp32, where the Neural Engine gives 0.9985 and the CPU 0.9805; meaning/ios-hdc, 2026-10-16)
    CpuAndNe,
}

pub struct Model(Retained<MLModel>);

impl Model {
    /// A value the converter wrote into the model's user-defined metadata.
    pub fn user_metadata(&self, key: &str) -> Option<String> {
        let meta = unsafe { self.0.modelDescription().metadata() };
        let creator = unsafe { objc2_core_ml::MLModelCreatorDefinedKey }?;
        let dict = meta.objectForKey(creator)?;
        let dict = dict.downcast::<NSDictionary>().ok()?;
        let v = dict.objectForKey(&*NSString::from_str(key))?;
        Some(v.downcast::<NSString>().ok()?.to_string())
    }
}

/// The models folder: $UGC_MODELS, else ~/host/models (where the apps' converted packages live).
pub fn models_dir() -> PathBuf {
    std::env::var_os("UGC_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("host/models"))
}

fn modified(p: &Path) -> std::time::SystemTime {
    // a package is a folder: its newest file decides
    fn walk(p: &Path, best: &mut std::time::SystemTime) {
        if let Ok(m) = std::fs::metadata(p).and_then(|m| m.modified()) {
            *best = (*best).max(m);
        }
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                walk(&e.path(), best);
            }
        }
    }
    let mut best = std::time::UNIX_EPOCH;
    walk(p, &mut best);
    best
}

impl Model {
    /// `folder/name.mlpackage` under the models folder, compiled into ~/Library/Caches/raea-perception when stale.
    pub fn load(folder: &str, name: &str, units: Units) -> Result<Model, String> {
        let package = models_dir().join(folder).join(format!("{name}.mlpackage"));
        if !package.exists() {
            return Err(format!("{}: not installed", package.display()));
        }
        let caches = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Caches/raea-perception");
        std::fs::create_dir_all(&caches).map_err(|e| e.to_string())?;
        let compiled = caches.join(format!("{name}.mlmodelc"));
        if !compiled.exists() || modified(&compiled) < modified(&package) {
            let url = NSURL::from_directory_path(&package).ok_or("bad package path")?;
            #[allow(deprecated)]
            let fresh = unsafe { MLModel::compileModelAtURL_error(&url) }.map_err(|e| format!("compiling {name}: {}", e.localizedDescription()))?;
            let from = fresh.to_file_path().ok_or("no compiled path")?;
            let _ = std::fs::remove_dir_all(&compiled);
            std::fs::rename(&from, &compiled).or_else(|_| copy_dir(&from, &compiled)).map_err(|e| e.to_string())?;
        }
        let config = unsafe { MLModelConfiguration::new() };
        // RAEA_COREML_UNITS=all|ne|gpu|cpu overrides every model's units, to measure one against another
        let units = match std::env::var("RAEA_COREML_UNITS").as_deref() {
            Ok("all") => Units::All,
            Ok("ne") => Units::CpuAndNe,
            Ok("gpu") => Units::CpuAndGpu,
            Ok("cpu") => Units::CpuOnly,
            _ => units,
        };
        unsafe {
            config.setComputeUnits(match units {
                Units::All => MLComputeUnits::All,
                Units::CpuAndGpu => MLComputeUnits::CPUAndGPU,
                Units::CpuOnly => MLComputeUnits::CPUOnly,
                Units::CpuAndNe => MLComputeUnits::CPUAndNeuralEngine,
            })
        };
        let url = NSURL::from_directory_path(&compiled).ok_or("bad compiled path")?;
        let m = unsafe { MLModel::modelWithContentsOfURL_configuration_error(&url, &config) }.map_err(|e| format!("loading {name}: {}", e.localizedDescription()))?;
        Ok(Model(m))
    }

    /// One prediction from named BGRA images (width, height, bytes row by row); the outputs by name.
    pub fn predict(&self, images: &[(&str, &Bgra)]) -> Result<Outputs, String> {
        let inputs: Vec<(&str, Input)> = images.iter().map(|(k, b)| (*k, Input::Image(b))).collect();
        self.predict_inputs(&inputs)
    }

    /// One prediction from named images and float arrays.
    pub fn predict_inputs(&self, inputs: &[(&str, Input)]) -> Result<Outputs, String> {
        let keys: Vec<Retained<NSString>> = inputs.iter().map(|(k, _)| NSString::from_str(k)).collect();
        let mut values: Vec<Retained<MLFeatureValue>> = Vec::new();
        for (_, input) in inputs {
            values.push(match input {
                Input::Image(img) => {
                    let px = img.pixel_buffer()?;
                    unsafe { MLFeatureValue::featureValueWithPixelBuffer(&px) }
                }
                Input::Array { shape, data } => {
                    let dims: Vec<Retained<NSNumber>> = shape.iter().map(|d| NSNumber::new_isize(*d as isize)).collect();
                    let a = unsafe { MLMultiArray::initWithShape_dataType_error(MLMultiArray::alloc(), &NSArray::from_retained_slice(&dims), MLMultiArrayDataType::Float32) }
                        .map_err(|e| format!("array: {}", e.localizedDescription()))?;
                    let strides: Vec<usize> = unsafe { a.strides() }.iter().map(|n| n.integerValue() as usize).collect();
                    #[allow(deprecated)]
                    let base = unsafe { a.dataPointer() }.as_ptr() as *mut f32;
                    let mut index = vec![0usize; shape.len()];
                    for v in data.iter() {
                        let offset: usize = index.iter().zip(&strides).map(|(i, s)| i * s).sum();
                        unsafe { *base.add(offset) = *v };
                        for d in (0..shape.len()).rev() {
                            index[d] += 1;
                            if index[d] < shape[d] {
                                break;
                            }
                            index[d] = 0;
                        }
                    }
                    unsafe { MLFeatureValue::featureValueWithMultiArray(&a) }
                }
            });
        }
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let value_refs: Vec<&AnyObject> = values.iter().map(|v| -> &AnyObject { v }).collect();
        let dict: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(&key_refs, &value_refs);
        let provider = unsafe { MLDictionaryFeatureProvider::initWithDictionary_error(MLDictionaryFeatureProvider::alloc(), &dict) }
            .map_err(|e| format!("inputs: {}", e.localizedDescription()))?;
        let out = unsafe { self.0.predictionFromFeatures_error(ProtocolObject::from_ref(&*provider)) }
            .map_err(|e| format!("prediction: {}", e.localizedDescription()))?;
        Ok(Outputs(out))
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() { copy_dir(&e.path(), &t)? } else { std::fs::copy(e.path(), t).map(|_| ())? }
    }
    Ok(())
}

/// A model input: a picture, or floats in row-major order with their shape.
pub enum Input<'a> {
    Image(&'a Bgra),
    Array { shape: Vec<usize>, data: Vec<f32> },
}

pub struct Outputs(Retained<ProtocolObject<dyn MLFeatureProvider>>);

impl Outputs {
    /// A multi-array output's values in logical (row-major) order, and its shape.
    pub fn floats(&self, name: &str) -> Result<(Vec<f32>, Vec<usize>), String> {
        let v = unsafe { self.0.featureValueForName(&NSString::from_str(name)) }.ok_or(format!("no output {name}"))?;
        let a: Retained<MLMultiArray> = unsafe { v.multiArrayValue() }.ok_or(format!("{name} isn't a multi-array"))?;
        let shape: Vec<usize> = unsafe { a.shape() }.iter().map(|n| n.integerValue() as usize).collect();
        let strides: Vec<usize> = unsafe { a.strides() }.iter().map(|n| n.integerValue() as usize).collect();
        let count: usize = shape.iter().product();
        let kind = unsafe { a.dataType() };
        #[allow(deprecated)]
        let base = unsafe { a.dataPointer() }.as_ptr();
        let mut out = vec![0f32; count];
        let mut index = vec![0usize; shape.len()];
        for o in out.iter_mut() {
            let offset: usize = index.iter().zip(&strides).map(|(i, s)| i * s).sum();
            *o = unsafe {
                if kind == MLMultiArrayDataType::Float32 {
                    *(base as *const f32).add(offset)
                } else if kind == MLMultiArrayDataType::Float16 {
                    f16_to_f32(*(base as *const u16).add(offset))
                } else if kind == MLMultiArrayDataType::Double {
                    *(base as *const f64).add(offset) as f32
                } else if kind == MLMultiArrayDataType::Int32 {
                    *(base as *const i32).add(offset) as f32
                } else {
                    return Err(format!("{name}: unsupported data type {:?}", kind.0));
                }
            };
            for d in (0..shape.len()).rev() {
                index[d] += 1;
                if index[d] < shape[d] {
                    break;
                }
                index[d] = 0;
            }
        }
        Ok((out, shape))
    }
}

fn f16_to_f32(h: u16) -> f32 {
    let (s, e, m) = ((h >> 15) as u32, ((h >> 10) & 0x1f) as u32, (h & 0x3ff) as u32);
    let bits = if e == 0 {
        if m == 0 {
            s << 31
        } else {
            // subnormal: renormalise
            let mut e = 127 - 15 + 1;
            let mut m = m;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (s << 31) | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if e == 31 {
        (s << 31) | (0xff << 23) | (m << 13)
    } else {
        (s << 31) | ((e + 127 - 15) << 23) | (m << 13)
    };
    f32::from_bits(bits)
}

/// A picture as BGRA bytes, row by row.
pub struct Bgra {
    pub w: usize,
    pub h: usize,
    pub bytes: Vec<u8>,
}

impl Bgra {
    pub(crate) fn pixel_buffer(&self) -> Result<CFRetained<CVPixelBuffer>, String> {
        let mut raw: *mut CVPixelBuffer = std::ptr::null_mut();
        let r = unsafe { CVPixelBufferCreate(None, self.w, self.h, kCVPixelFormatType_32BGRA, None, NonNull::new(&mut raw).unwrap()) };
        if r != 0 || raw.is_null() {
            return Err(format!("CVPixelBufferCreate: {r}"));
        }
        let px = unsafe { CFRetained::from_raw(NonNull::new(raw).unwrap()) };
        unsafe { CVPixelBufferLockBaseAddress(&px, CVPixelBufferLockFlags(0)) };
        let base = CVPixelBufferGetBaseAddress(&px) as *mut u8;
        let row = CVPixelBufferGetBytesPerRow(&px);
        for y in 0..self.h {
            unsafe { std::ptr::copy_nonoverlapping(self.bytes.as_ptr().add(y * self.w * 4), base.add(y * row), self.w * 4) };
        }
        unsafe { CVPixelBufferUnlockBaseAddress(&px, CVPixelBufferLockFlags(0)) };
        Ok(px)
    }
}
