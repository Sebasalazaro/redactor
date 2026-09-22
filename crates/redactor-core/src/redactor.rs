//! The public entry point: [`Redactor`].

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::config::{Config, MaskingConfig};
use crate::detectors;
use crate::dictionary::Dictionary;
use crate::finding::Finding;
use crate::format::{self, InputFormat};
use crate::{mask, tld};

/// Nested content (JWT claims, decoded values) is redacted recursively up to
/// this depth; anything deeper is treated as an opaque secret.
const MAX_DEPTH: usize = 3;

/// Header names whose values are always secrets.
const SENSITIVE_HEADERS: &[&str] = &[
    "api-key",
    "apikey",
    "authorization",
    "cookie",
    "csrf-token",
    "ocp-apim-subscription-key",
    "private-token",
    "proxy-authorization",
    "set-cookie",
    "token",
    "x-access-token",
    "x-amz-security-token",
    "x-api-key",
    "x-auth-token",
    "x-client-secret",
    "x-csrf-token",
    "x-csrftoken",
    "x-goog-api-key",
    "x-hub-signature",
    "x-hub-signature-256",
    "x-id-token",
    "x-refresh-token",
    "x-session-id",
    "x-session-token",
    "x-signature",
    "x-token",
    "x-vault-token",
    "x-xsrf-token",
];

/// Domains left untouched unless they contain the client's name.
const DEFAULT_ALLOWED_DOMAINS: &[&str] = &[
    "cloudflare.com",
    "example.com",
    "example.net",
    "example.org",
    "github.com",
    "google.com",
    "googleapis.com",
    "gstatic.com",
    "jsdelivr.net",
    "mozilla.org",
    "schema.org",
    "unpkg.com",
    "w3.org",
];

/// Result of redacting one input.
#[derive(Debug, Clone, Serialize)]
pub struct Redaction {
    pub output: String,
    pub format: InputFormat,
    pub findings: Vec<Finding>,
    /// The normalized input the findings point into. Never serialized.
    #[serde(skip)]
    pub input: String,
}

/// A piece of a [`Redaction`]: untouched text or a redacted value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
    Plain(&'a str),
    Redacted(&'a Finding),
}

impl Redaction {
    /// The input split into untouched text and findings, in order. Joining
    /// plain text with each finding's replacement yields [`Self::output`];
    /// joining it with the originals yields the input.
    pub fn segments(&self) -> Vec<Segment<'_>> {
        let mut segments = Vec::with_capacity(self.findings.len() * 2 + 1);
        let mut cursor = 0;
        for finding in &self.findings {
            if finding.start > cursor {
                segments.push(Segment::Plain(&self.input[cursor..finding.start]));
            }
            segments.push(Segment::Redacted(finding));
            cursor = finding.end;
        }
        if cursor < self.input.len() {
            segments.push(Segment::Plain(&self.input[cursor..]));
        }
        segments
    }
}

/// A compiled redaction policy. Build it once and reuse it: construction
/// compiles the term dictionary, redaction itself is a few linear passes.
pub struct Redactor {
    pub(crate) masking: MaskingConfig,
    pub(crate) dict: Dictionary,
    sensitive_headers: HashSet<String>,
    pub(crate) sensitive_keys: HashSet<String>,
    allow_domains: Vec<String>,
}

impl Redactor {
    pub fn new(config: &Config) -> Self {
        Self {
            masking: config.masking(),
            dict: Dictionary::new(&config.client, &config.users, &config.hosts, &config.terms),
            sensitive_headers: SENSITIVE_HEADERS
                .iter()
                .map(|h| h.to_string())
                .chain(
                    config
                        .sensitive_headers
                        .iter()
                        .map(|h| h.to_ascii_lowercase()),
                )
                .collect(),
            sensitive_keys: config
                .sensitive_keys
                .iter()
                .map(|k| normalize_key(k))
                .collect(),
            allow_domains: DEFAULT_ALLOWED_DOMAINS
                .iter()
                .map(|d| d.to_string())
                .chain(config.allow_domains.iter().map(|d| d.to_ascii_lowercase()))
                .collect(),
        }
    }

    /// Redacts `input`, detecting its format first.
    pub fn redact(&self, input: &str) -> Redaction {
        let format = InputFormat::detect(input);
        let normalized = format::normalize(input, format);
        let (output, findings) = self.redact_at(&normalized, 0);
        Redaction {
            output,
            format,
            findings,
            input: normalized.into_owned(),
        }
    }

    /// Redacts a fragment found inside another value (e.g. a JWT claim).
    pub(crate) fn redact_nested(&self, text: &str, depth: usize) -> String {
        if depth > MAX_DEPTH {
            return self.mask_secret(text);
        }
        self.redact_at(text, depth).0
    }

    fn redact_at(&self, text: &str, depth: usize) -> (String, Vec<Finding>) {
        let findings = resolve(detectors::scan(self, text, depth));
        (apply(text, &findings), findings)
    }

    pub(crate) fn is_sensitive_header(&self, name: &str) -> bool {
        self.sensitive_headers.contains(&name.to_ascii_lowercase())
    }

    // Masking helpers. They all protect client names first, so a partially
    // visible value never shows the client.

    /// Masks `value` as a secret with this policy's ratio. Used for values
    /// the user marks by hand.
    pub fn mask_secret(&self, value: &str) -> String {
        let masked = mask::secret(value, self.masking.secret_keep);
        self.dict.replace_clients(&masked).unwrap_or(masked)
    }

    pub(crate) fn mask_pii(&self, value: &str) -> String {
        let keep = self.masking.pii_keep;
        self.dict
            .mask_around_clients(value, |s| mask::keep_head(s, keep))
    }

    pub(crate) fn mask_id(&self, value: &str) -> String {
        mask::cut_tail(value, self.masking.id_cut)
    }

    pub(crate) fn mask_email(&self, email: &str) -> String {
        match email.rsplit_once('@') {
            Some((local, domain)) => format!("{}@{}", self.mask_pii(local), self.mask_host(domain)),
            None => self.mask_pii(email),
        }
    }

    /// Masks a hostname: client names become `[CLIENT]`, the registrable
    /// label is partially hidden and the public suffix stays visible
    /// (`api.acme-payments.com` → `api.ac**-paym****.com`).
    pub(crate) fn mask_host(&self, host: &str) -> String {
        if self.is_allowed_domain(host) {
            return self
                .dict
                .replace_clients(host)
                .unwrap_or_else(|| host.to_string());
        }
        let labels: Vec<&str> = host.split('.').collect();
        let suffix_len = match labels.len() {
            0 | 1 => 0,
            n if tld::is_second_level_suffix(&labels[n - 2..].join(".").to_ascii_lowercase()) => {
                2.min(n - 1)
            }
            _ => 1,
        };
        let registrable = labels.len() - suffix_len - 1;
        labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                if let Some(replaced) = self.dict.replace_clients(label) {
                    replaced
                } else if i == registrable {
                    mask::keep_head(label, self.masking.pii_keep)
                } else {
                    label.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(".")
    }

    fn is_allowed_domain(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.allow_domains.iter().any(|d| {
            host == *d
                || host
                    .strip_suffix(d.as_str())
                    .is_some_and(|p| p.ends_with('.'))
        })
    }
}

/// Lowercases a key and drops separators: `Client_Secret` → `clientsecret`.
pub(crate) fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Picks a non-overlapping subset of findings: higher priority categories
/// first, then longer spans. Returned sorted by position.
fn resolve(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then(b.len().cmp(&a.len()))
            .then(a.start.cmp(&b.start))
    });
    // start -> finding; accepted spans never overlap, so only the closest
    // span starting before `end` can collide with a candidate.
    let mut accepted: BTreeMap<usize, Finding> = BTreeMap::new();
    for finding in findings {
        if finding.is_empty() {
            continue;
        }
        let collides = accepted
            .range(..finding.end)
            .next_back()
            .is_some_and(|(_, prev)| prev.end > finding.start);
        if !collides {
            accepted.insert(finding.start, finding);
        }
    }
    accepted.into_values().collect()
}

fn apply(text: &str, findings: &[Finding]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for f in findings {
        out.push_str(&text[cursor..f.start]);
        out.push_str(&f.replacement);
        cursor = f.end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Category;

    fn finding(start: usize, end: usize, category: Category) -> Finding {
        Finding {
            start,
            end,
            category,
            original: String::new(),
            replacement: String::new(),
        }
    }

    #[test]
    fn resolve_prefers_priority_then_length() {
        let kept = resolve(vec![
            finding(0, 10, Category::Client),
            finding(2, 5, Category::Secret),
            finding(20, 30, Category::Host),
            finding(22, 25, Category::Client),
            finding(40, 45, Category::Id),
            finding(40, 50, Category::Id),
        ]);
        let spans: Vec<_> = kept.iter().map(|f| (f.start, f.end)).collect();
        assert_eq!(spans, [(2, 5), (20, 30), (40, 50)]);
    }

    #[test]
    fn segments_rebuild_output_and_input() {
        let r = Redactor::new(&Config {
            client: vec!["Globex".into()],
            ..Default::default()
        });
        let redaction = r.redact("GET /users/88127 HTTP/1.1\nHost: api.globex.example\n");
        let (mut output, mut input) = (String::new(), String::new());
        for segment in redaction.segments() {
            match segment {
                Segment::Plain(text) => {
                    output.push_str(text);
                    input.push_str(text);
                }
                Segment::Redacted(f) => {
                    output.push_str(&f.replacement);
                    input.push_str(&f.original);
                }
            }
        }
        assert_eq!(output, redaction.output);
        assert_eq!(input, redaction.input);
        assert_eq!(redaction.findings.len(), 2);
    }

    #[test]
    fn masks_hosts() {
        let r = Redactor::new(&Config {
            client: vec!["Globex Bank".into()],
            ..Default::default()
        });
        assert_eq!(
            r.mask_host("api.globexbank.example"),
            "api.[CLIENT].example"
        );
        assert_eq!(
            r.mask_host("api.acme-payments.com"),
            "api.ac**-paym****.com"
        );
        assert_eq!(
            r.mask_host("portal.initech.com.co"),
            "portal.ini****.com.co"
        );
        assert_eq!(r.mask_host("srv-db01"), "sr*-db**");
        assert_eq!(r.mask_host("fonts.googleapis.com"), "fonts.googleapis.com");
    }
}
