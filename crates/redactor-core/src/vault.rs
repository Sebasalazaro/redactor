//! Reversible mode: remembers which original value hides behind each
//! redacted value, so an LLM's answer can be turned back into something
//! usable (`curl https://api.[CLIENT].example/...` -> the real host).
//!
//! A vault belongs to one profile (an engagement, or the global config). It
//! holds real values, so it is only ever written to disk encrypted with
//! AES-256-GCM; the profile id is bound to the ciphertext as associated
//! data. The key comes from the caller, e.g. the OS keychain
//! (see [`crate::keychain`]).
//!
//! Redaction is lossy: `1042` and `1043` both become `104*`. When a
//! redacted value maps to several originals, [`Vault::restore`] does not
//! guess; it reports the candidates.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use aho_corasick::{AhoCorasick, MatchKind};
use serde::{Deserialize, Serialize};

use crate::redactor::Redaction;

/// Header of an encrypted vault file (format version 1).
const MAGIC: &[u8; 4] = b"RDV1";
const NONCE_LEN: usize = 12;
/// Oldest mappings are dropped beyond this, so a vault cannot grow forever.
const MAX_ENTRIES: usize = 20_000;

/// A 256-bit vault key.
pub type VaultKey = [u8; 32];

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("not a redactor vault")]
    Format,
    #[error("cannot decrypt the vault (wrong key, other profile, or damaged file)")]
    Decrypt,
    #[error("cannot encrypt the vault")]
    Encrypt,
    #[error("vault contents: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("vault storage: {0}")]
    Storage(String),
    #[error("keychain: {0}")]
    Keychain(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Vault {
    /// Redacted value -> the originals seen behind it.
    entries: BTreeMap<String, Vec<Original>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Original {
    value: String,
    uses: u64,
    /// Unix seconds, for pruning.
    last_used: u64,
}

/// One piece of a restored text.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RestorePart {
    Plain {
        text: String,
    },
    Restored {
        original: String,
        replacement: String,
    },
    /// Several originals share this redacted value; it is left as is.
    Ambiguous {
        replacement: String,
        candidates: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Restoration {
    pub parts: Vec<RestorePart>,
}

impl Restoration {
    /// The restored text; ambiguous values keep their redacted form.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                RestorePart::Plain { text } => text.as_str(),
                RestorePart::Restored { original, .. } => original.as_str(),
                RestorePart::Ambiguous { replacement, .. } => replacement.as_str(),
            })
            .collect()
    }

    pub fn restored(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| matches!(p, RestorePart::Restored { .. }))
            .count()
    }

    pub fn ambiguous(&self) -> usize {
        self.parts
            .iter()
            .filter(|p| matches!(p, RestorePart::Ambiguous { .. }))
            .count()
    }
}

impl Vault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of distinct redacted values remembered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remembers that `original` was shared as `replacement`.
    pub fn record(&mut self, replacement: &str, original: &str) {
        if replacement.is_empty() || replacement == original {
            return;
        }
        let now = now();
        let originals = self.entries.entry(replacement.to_string()).or_default();
        match originals.iter_mut().find(|o| o.value == original) {
            Some(o) => {
                o.uses += 1;
                o.last_used = now;
            }
            None => originals.push(Original {
                value: original.to_string(),
                uses: 1,
                last_used: now,
            }),
        }
        if self.entries.len() > MAX_ENTRIES {
            self.prune(MAX_ENTRIES);
        }
    }

    /// Remembers every finding of a redaction.
    pub fn record_redaction(&mut self, redaction: &Redaction) {
        for finding in &redaction.findings {
            self.record(&finding.replacement, redaction.original(finding));
        }
    }

    /// Replaces every remembered redacted value in `text` with its original.
    pub fn restore(&self, text: &str) -> Restoration {
        let mut parts = Vec::new();
        if self.entries.is_empty() {
            if !text.is_empty() {
                parts.push(RestorePart::Plain {
                    text: text.to_string(),
                });
            }
            return Restoration { parts };
        }
        let keys: Vec<&String> = self.entries.keys().collect();
        let automaton = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&keys)
            .expect("literal patterns always compile");

        let mut cursor = 0;
        for m in automaton.find_iter(text) {
            if m.start() > cursor {
                parts.push(RestorePart::Plain {
                    text: text[cursor..m.start()].to_string(),
                });
            }
            let replacement = keys[m.pattern().as_usize()];
            let originals = &self.entries[replacement];
            parts.push(match originals.as_slice() {
                [only] => RestorePart::Restored {
                    original: only.value.clone(),
                    replacement: replacement.clone(),
                },
                many => {
                    let mut sorted: Vec<&Original> = many.iter().collect();
                    sorted.sort_by(|a, b| b.uses.cmp(&a.uses).then(b.last_used.cmp(&a.last_used)));
                    RestorePart::Ambiguous {
                        replacement: replacement.clone(),
                        candidates: sorted.iter().map(|o| o.value.clone()).collect(),
                    }
                }
            });
            cursor = m.end();
        }
        if cursor < text.len() {
            parts.push(RestorePart::Plain {
                text: text[cursor..].to_string(),
            });
        }
        Restoration { parts }
    }

    /// Keeps the `max` most recently used redacted values.
    pub fn prune(&mut self, max: usize) {
        if self.entries.len() <= max {
            return;
        }
        let mut by_age: Vec<(u64, String)> = self
            .entries
            .iter()
            .map(|(k, v)| (v.iter().map(|o| o.last_used).max().unwrap_or(0), k.clone()))
            .collect();
        by_age.sort();
        let excess = self.entries.len() - max;
        for (_, key) in by_age.into_iter().take(excess) {
            self.entries.remove(&key);
        }
    }

    /// Encrypts the vault for `profile` (bound as associated data).
    pub fn seal(&self, key: &VaultKey, profile: &str) -> Result<Vec<u8>, VaultError> {
        let cipher = Aes256Gcm::new(key.into());
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let plaintext = serde_json::to_vec(self)?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: profile.as_bytes(),
                },
            )
            .map_err(|_| VaultError::Encrypt)?;
        let mut out = Vec::with_capacity(MAGIC.len() + NONCE_LEN + ciphertext.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts a vault sealed for `profile`.
    pub fn open(bytes: &[u8], key: &VaultKey, profile: &str) -> Result<Self, VaultError> {
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or(VaultError::Format)?;
        if body.len() < NONCE_LEN {
            return Err(VaultError::Format);
        }
        let (nonce, ciphertext) = body.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new(key.into());
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: profile.as_bytes(),
                },
            )
            .map_err(|_| VaultError::Decrypt)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }
}

/// A new random vault key.
pub fn generate_key() -> VaultKey {
    Aes256Gcm::generate_key(&mut OsRng).into()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Redactor};

    fn vault() -> Vault {
        let mut v = Vault::new();
        v.record("api.[CLIENT].example", "api.globexbank.example");
        v.record("a7f3e9c1d…[len=32]", "a7f3e9c1d5b2084f6e1a9c3d7b5f2e80");
        v.record("104*", "1042");
        v.record("104*", "1043");
        v.record("104*", "1043");
        v
    }

    #[test]
    fn restores_unique_values_and_flags_ambiguous_ones() {
        let restored = vault()
            .restore("curl -b 'sid=a7f3e9c1d…[len=32]' https://api.[CLIENT].example/users/104*");
        assert_eq!(
            restored.text(),
            "curl -b 'sid=a7f3e9c1d5b2084f6e1a9c3d7b5f2e80' https://api.globexbank.example/users/104*"
        );
        assert_eq!((restored.restored(), restored.ambiguous()), (2, 1));
        assert!(restored.parts.contains(&RestorePart::Ambiguous {
            replacement: "104*".into(),
            candidates: vec!["1043".into(), "1042".into()], // most used first
        }));
    }

    #[test]
    fn records_whole_redactions() {
        let redactor = Redactor::new(&Config {
            client: vec!["Globex Bank".into()],
            ..Default::default()
        });
        let input = "GET /users/88127 HTTP/1.1\nHost: api.globexbank.example\n";
        let redaction = redactor.redact(input);
        let mut vault = Vault::new();
        vault.record_redaction(&redaction);
        assert_eq!(vault.restore(&redaction.output).text(), input);
    }

    #[test]
    fn sealed_vaults_need_the_key_and_the_profile() {
        let key = generate_key();
        let sealed = vault().seal(&key, "globex-q3").unwrap();
        assert!(!sealed.windows(4).any(|w| w == b"1042"), "plaintext leaked");
        assert_eq!(Vault::open(&sealed, &key, "globex-q3").unwrap(), vault());
        assert!(matches!(
            Vault::open(&sealed, &generate_key(), "globex-q3"),
            Err(VaultError::Decrypt)
        ));
        assert!(matches!(
            Vault::open(&sealed, &key, "initech"),
            Err(VaultError::Decrypt)
        ));

        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(matches!(
            Vault::open(&tampered, &key, "globex-q3"),
            Err(VaultError::Decrypt)
        ));
        assert!(matches!(
            Vault::open(b"nope", &key, "globex-q3"),
            Err(VaultError::Format)
        ));
    }

    #[test]
    fn prunes_oldest_entries() {
        let mut v = Vault::new();
        for i in 0..10 {
            v.record(&format!("r{i}"), &format!("o{i}"));
            v.entries.get_mut(&format!("r{i}")).unwrap()[0].last_used = i;
        }
        v.prune(3);
        assert_eq!(v.entries.keys().collect::<Vec<_>>(), ["r7", "r8", "r9"]);
    }
}
