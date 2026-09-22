//! Optional, fully local named-entity recognition for redactor.
//!
//! Runs [GLiNER](https://github.com/urchade/GLiNER) models exported to ONNX
//! with ONNX Runtime. GLiNER is zero-shot: the entity types are plain-text
//! labels chosen at call time (`"name"`, `"organization"`, ...). It catches
//! what patterns cannot, such as a person's name in free text.
//!
//! Only token-level models are supported (`span_mode = "token_level"` in
//! `gliner_config.json`), such as `knowledgator/gliner-pii-edge-v1.0`. The
//! model files are never downloaded by this crate: see
//! `scripts/fetch-model.sh`.
//!
//! ONNX Runtime keeps much of the memory it used even after a model is
//! dropped, so applications should run it in a separate process that they
//! can end when idle: see [`worker`].

mod decode;
mod model;
mod words;
pub mod worker;

pub use decode::Entity;
pub use model::{Model, NerError};

/// Labels used by redactor, in the vocabulary the PII models were trained on.
pub const DEFAULT_LABELS: &[&str] = &[
    "name",
    "organization",
    "username",
    "password",
    "location address",
    "phone number",
];

/// Minimum score for an entity to be reported, as the model card suggests.
/// Findings are always shown for review, so recall matters more here.
pub const DEFAULT_THRESHOLD: f32 = 0.3;
