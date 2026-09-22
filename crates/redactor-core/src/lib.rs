//! Offline redaction engine for pentest traffic.
//!
//! `redactor-core` takes raw HTTP requests, cURL commands, HAR files, Burp
//! exports or plain logs and rewrites the sensitive parts (client names,
//! hosts, tokens, cookies, identifiers, PII) so the result can be shared with
//! an LLM without leaking engagement data. Nothing here touches the network.

pub mod config;
mod dictionary;
pub mod mask;

pub use config::{Config, ConfigError, CustomTerm, MaskingConfig};
