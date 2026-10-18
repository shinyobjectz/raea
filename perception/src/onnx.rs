//! ONNX sessions, on ONNX Runtime's CPU kernels or handed to Core ML (CPU and Neural Engine, never the GPU: SigLIP on
//! the M5 Pro's GPU is wrong, memory note coreml-gpu-siglip) through ONNX Runtime's Core ML execution provider. Which
//! models go to Core ML is measured per model (speed, and agreement with the CPU reference) before it's the default:
//! `RAEA_ORT_COREML=all` sends every model, `RAEA_ORT_COREML=htdemucs,transnet` the ones named, `RAEA_ORT_COREML=none`
//! none; unset, the measured list below.

use ort::session::Session;

/// models measured faster on Core ML with the same outputs (2026-10-17); each entry says by how much
pub const COREML_DEFAULT: [&str; 0] = [];

fn wants(name: &str) -> bool {
    match std::env::var("RAEA_ORT_COREML") {
        Ok(v) if v == "all" => true,
        Ok(v) if v == "none" => false,
        Ok(v) => v.split(',').any(|x| x.trim() == name),
        Err(_) => COREML_DEFAULT.contains(&name),
    }
}

/// a session for `path`, named `name` for the Core ML choice above
pub fn session(name: &str, path: impl AsRef<std::path::Path>) -> ort::Result<Session> {
    let mut b = Session::builder()?;
    if wants(name) {
        use ort::ep::coreml::{ComputeUnits, CoreML, ModelFormat};
        let cache = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-coreml");
        let _ = std::fs::create_dir_all(&cache);
        b = b.with_execution_providers([CoreML::default()
            .with_compute_units(ComputeUnits::CPUAndNeuralEngine)
            .with_model_format(ModelFormat::MLProgram)
            .with_model_cache_dir(cache.display().to_string())
            .build()])?;
    }
    b.commit_from_file(path)
}
