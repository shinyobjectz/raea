//! The voice: four models on ONNX Runtime and the turn-taking between them (void.feature, "Talk to it").
//!
//! In: the mic at 16 kHz, through Silero VAD v5 (who is talking) and Smart Turn v3 (has the thought finished, judged at
//! each pause), then Parakeet TDT 0.6B v3 for the words. Out: Pocket TTS (Kyutai, 100M), streamed 80 ms frames at
//! 24 kHz. The turn rules are the host app's (~/host/docs/voice-mode.feature), measured there on the phone.
//!
//! Every model is an ONNX export run by `ort`, like raea's other models; weights download once into
//! ~/.cache/raea-void/models (models.rs).

pub mod models;
pub mod vad;
pub mod turn;
pub mod stt;
pub mod tts;
pub mod mel;
pub mod audio;
pub mod turns;
pub mod runtime;

pub use stt::Parakeet;
pub use tts::Pocket;
pub use turn::SmartTurn;
pub use vad::Vad;

/// The rate every listening model wants.
pub const LISTEN_RATE: u32 = 16_000;

/// One ort session the way raea builds them (ai/meaning/embed): full graph optimisation, a few intra-op threads (raea-ml).
pub use raea_ml::session;
