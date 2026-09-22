//! Configuration model.
//!
//! A redactor is configured in two layers:
//!
//! * a **global** config with what rarely changes (the client's name and its
//!   variants, allow-listed public domains, masking ratios), and
//! * an optional **engagement** profile with what changes per test (in-scope
//!   hosts, test users, device names, extra keywords).
//!
//! [`Config::with_engagement`] merges both: lists are concatenated and the
//! engagement's masking ratios, when present, win.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Rules that decide what gets redacted and how.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Human readable name of the profile (e.g. the engagement id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Client names. Each one is replaced by `[CLIENT]` wherever it appears,
    /// including common spellings (`Globex Bank` → `globex-bank`, `GlobexBank`, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub client: Vec<String>,
    /// Usernames or people that must be partially masked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<String>,
    /// Hosts in scope. Useful for internal names that are not FQDNs (`srv-db01`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    /// Arbitrary terms with an explicit replacement (device names, project codenames).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<CustomTerm>,
    /// Public domains that are never masked (CDNs, standards bodies, ...).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow_domains: Vec<String>,
    /// Extra HTTP header names whose values are secrets.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sensitive_headers: Vec<String>,
    /// Extra JSON / query / form keys whose values are secrets.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sensitive_keys: Vec<String>,
    /// How much of each value stays visible. Defaults apply when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masking: Option<MaskingConfig>,
}

/// A literal term with its replacement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomTerm {
    pub value: String,
    /// Defaults to `[REDACTED]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

/// Ratios that control partial masking. All values are between 0 and 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MaskingConfig {
    /// Fraction of an identifier that is hidden (`1042` → `104*` with 0.2).
    pub id_cut: f64,
    /// Fraction of a secret that stays visible as a prefix.
    pub secret_keep: f64,
    /// Fraction of each word of personal data that stays visible.
    pub pii_keep: f64,
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            id_cut: 0.2,
            secret_keep: 0.3,
            pii_keep: 0.4,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("masking.{field} = {value} is out of range (expected 0.0..=1.0)")]
    Ratio { field: &'static str, value: f64 },
    #[error("cannot serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("invalid engagement name {0:?} (use letters, digits, '-', '_' or '.')")]
    InvalidName(String),
}

impl Config {
    /// Parses and validates a TOML document.
    pub fn from_toml(source: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(source)?;
        config.validate()?;
        Ok(config)
    }

    /// Reads a TOML config from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&source)
    }

    /// Serializes to TOML, omitting empty fields.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.validate()?;
        Ok(toml::to_string_pretty(self)?)
    }

    /// Returns a new config with `engagement` layered on top of `self`.
    pub fn with_engagement(&self, engagement: &Config) -> Config {
        fn concat<T: Clone>(a: &[T], b: &[T]) -> Vec<T> {
            a.iter().chain(b).cloned().collect()
        }
        Config {
            name: engagement.name.clone().or_else(|| self.name.clone()),
            client: concat(&self.client, &engagement.client),
            users: concat(&self.users, &engagement.users),
            hosts: concat(&self.hosts, &engagement.hosts),
            terms: concat(&self.terms, &engagement.terms),
            allow_domains: concat(&self.allow_domains, &engagement.allow_domains),
            sensitive_headers: concat(&self.sensitive_headers, &engagement.sensitive_headers),
            sensitive_keys: concat(&self.sensitive_keys, &engagement.sensitive_keys),
            masking: engagement.masking.or(self.masking),
        }
    }

    /// Effective masking ratios.
    pub fn masking(&self) -> MaskingConfig {
        self.masking.unwrap_or_default()
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let m = self.masking();
        for (field, value) in [
            ("id_cut", m.id_cut),
            ("secret_keep", m.secret_keep),
            ("pii_keep", m.pii_keep),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(ConfigError::Ratio { field, value });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let config = Config::from_toml(
            r#"
            name = "globex-q3"
            client = ["Globex Bank"]
            hosts = ["srv-db01"]

            [[terms]]
            value = "MacBook-de-Pentester"
            replacement = "[DEVICE]"

            [masking]
            id_cut = 0.25
            "#,
        )
        .unwrap();

        assert_eq!(config.client, ["Globex Bank"]);
        assert_eq!(config.terms[0].replacement.as_deref(), Some("[DEVICE]"));
        assert_eq!(config.masking().id_cut, 0.25);
        assert_eq!(config.masking().secret_keep, 0.3);
    }

    #[test]
    fn rejects_unknown_fields_and_bad_ratios() {
        assert!(Config::from_toml("clients = []").is_err());
        assert!(matches!(
            Config::from_toml("[masking]\npii_keep = 1.5"),
            Err(ConfigError::Ratio {
                field: "pii_keep",
                ..
            })
        ));
    }

    #[test]
    fn serializes_compactly() {
        let config = Config {
            client: vec!["Globex".into()],
            ..Default::default()
        };
        let toml = config.to_toml().unwrap();
        assert_eq!(toml.trim(), r#"client = ["Globex"]"#);
        assert_eq!(Config::from_toml(&toml).unwrap(), config);
    }

    #[test]
    fn engagement_extends_global() {
        let global = Config {
            client: vec!["Globex".into()],
            masking: Some(MaskingConfig::default()),
            ..Default::default()
        };
        let engagement = Config {
            name: Some("q3".into()),
            hosts: vec!["api.globex.example".into()],
            ..Default::default()
        };

        let merged = global.with_engagement(&engagement);
        assert_eq!(merged.client, ["Globex"]);
        assert_eq!(merged.hosts, ["api.globex.example"]);
        assert_eq!(merged.name.as_deref(), Some("q3"));
        assert_eq!(merged.masking, Some(MaskingConfig::default()));
    }
}
