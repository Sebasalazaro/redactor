//! Offline redaction engine for pentest traffic.
//!
//! `redactor-core` takes raw HTTP requests, cURL commands, HAR files, Burp
//! exports or plain logs and rewrites the sensitive parts (client names,
//! hosts, tokens, cookies, identifiers, PII) so the result can be shared with
//! an LLM without leaking engagement data. Nothing here touches the network.
//!
//! ```
//! use redactor_core::{Config, Redactor};
//!
//! let config = Config::from_toml(r#"client = ["Globex Bank"]"#).unwrap();
//! let redactor = Redactor::new(&config);
//! let result = redactor.redact("GET /accounts/88127 HTTP/1.1\nHost: api.globexbank.example\n");
//! assert_eq!(result.output, "GET /accounts/8812* HTTP/1.1\nHost: api.[CLIENT].example\n");
//! ```

pub mod config;
mod detectors;
mod dictionary;
pub mod finding;
pub mod format;
mod jwt;
pub mod mask;
mod redactor;
mod tld;

pub use config::{Config, ConfigError, CustomTerm, MaskingConfig};
pub use dictionary::{CLIENT_PLACEHOLDER, DEFAULT_TERM_PLACEHOLDER};
pub use finding::{Category, Finding};
pub use format::InputFormat;
pub use redactor::{Redaction, Redactor};
