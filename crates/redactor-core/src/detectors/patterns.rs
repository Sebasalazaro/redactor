//! Format-independent patterns: keys, tokens, emails, network identifiers.

use std::sync::LazyLock;

use regex::Regex;

use super::Scan;
use crate::finding::Category;
use crate::{mask, tld};

static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"-----BEGIN (?P<kind>[A-Z0-9 ]*)PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
    )
    .unwrap()
});

static JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{5,}\.eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]*").unwrap()
});

/// Vendor tokens with a recognizable prefix.
static KNOWN_TOKENS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",          // AWS access key id
        r"\bgh[pousr]_[A-Za-z0-9]{36,}\b",         // GitHub
        r"\bgithub_pat_[A-Za-z0-9_]{22,}\b",       // GitHub fine-grained
        r"\bglpat-[A-Za-z0-9_-]{20,}\b",           // GitLab
        r"\b[sr]k_(?:live|test)_[A-Za-z0-9]{16,}", // Stripe
        r"\bsk-[A-Za-z0-9_-]{20,}",                // OpenAI / Anthropic style
        r"\bxox[abpors]-[A-Za-z0-9-]{10,}",        // Slack
        r"\bAIza[0-9A-Za-z_-]{35}\b",              // Google API key
        r"\bya29\.[0-9A-Za-z_-]{20,}",             // Google OAuth access token
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b[A-Za-z0-9._%+-]+@(?:[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?\.)+[A-Za-z]{2,24}\b",
    )
    .unwrap()
});

/// 13–19 digits, optionally grouped by spaces or dashes; confirmed with Luhn.
static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d(?:[ -]?\d){12,18}\b").unwrap());

static UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b")
        .unwrap()
});

static IPV4: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\b",
    )
    .unwrap()
});

static DOMAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?\.)+(?P<tld>[A-Za-z]{2,24})\b")
        .unwrap()
});

/// Numeric path segments: `/users/1042/orders`.
static PATH_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/(?P<id>\d{2,})\b").unwrap());

/// Credentials embedded in URLs: `https://user:pass@host`.
static URL_CREDENTIALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[a-z][a-z0-9+.-]*://(?P<user>[^\s:/@'\x22]+):(?P<pass>[^\s@/'\x22]+)@")
        .unwrap()
});

pub(super) fn detect(scan: &mut Scan) {
    let r = scan.r;
    let text = scan.text;

    for caps in PRIVATE_KEY.captures_iter(text) {
        let m = caps.get(0).unwrap();
        let kind = &caps["kind"];
        let replacement =
            format!("-----BEGIN {kind}PRIVATE KEY-----[REDACTED]-----END {kind}PRIVATE KEY-----");
        scan.push(m.start(), m.end(), Category::PrivateKey, replacement);
    }

    for m in JWT.find_iter(text) {
        scan.token(m.start(), m.end());
    }

    for re in KNOWN_TOKENS.iter() {
        for m in re.find_iter(text) {
            scan.push(
                m.start(),
                m.end(),
                Category::Secret,
                r.mask_secret(m.as_str()),
            );
        }
    }

    for caps in URL_CREDENTIALS.captures_iter(text) {
        let (user, pass) = (caps.name("user").unwrap(), caps.name("pass").unwrap());
        scan.push(
            user.start(),
            user.end(),
            Category::Credential,
            r.mask_pii(user.as_str()),
        );
        scan.push(
            pass.start(),
            pass.end(),
            Category::Credential,
            r.mask_secret(pass.as_str()),
        );
    }

    for m in EMAIL.find_iter(text) {
        scan.push(
            m.start(),
            m.end(),
            Category::Email,
            r.mask_email(m.as_str()),
        );
    }

    for m in CARD.find_iter(text) {
        if mask::luhn_valid(m.as_str()) {
            scan.push(m.start(), m.end(), Category::Card, mask::card(m.as_str()));
        }
    }

    for m in UUID.find_iter(text) {
        scan.push(m.start(), m.end(), Category::Uuid, r.mask_id(m.as_str()));
    }

    for m in IPV4.find_iter(text) {
        if is_ip_context(text, m.start(), m.end()) {
            scan.push(m.start(), m.end(), Category::Ip, mask::ipv4(m.as_str()));
        }
    }

    for caps in DOMAIN.captures_iter(text) {
        let m = caps.get(0).unwrap();
        if tld::is_known(&caps["tld"].to_ascii_lowercase()) && !continues_dotted(text, m.end()) {
            scan.push(m.start(), m.end(), Category::Host, r.mask_host(m.as_str()));
        }
    }

    for caps in PATH_ID.captures_iter(text) {
        let id = caps.name("id").unwrap();
        let slash = id.start() - 1;
        // `2026/09/22` (dates, CIDR) and `Chrome/120.0` (versions) are not ids.
        let after_digit = text[..slash].ends_with(|c: char| c.is_ascii_digit());
        let is_version = continues_dotted(text, id.end());
        if !after_digit && !is_version {
            scan.push(id.start(), id.end(), Category::Id, r.mask_id(id.as_str()));
        }
    }
}

/// Filters version strings (`Chrome/120.0.0.0`, `1.2.3.4.5`) out of IPv4 matches.
fn is_ip_context(text: &str, start: usize, end: usize) -> bool {
    let before = &text[..start];
    let after = &text[end..];
    if matches!(before.chars().next_back(), Some('.')) || after.starts_with('.') {
        return false;
    }
    if before.ends_with('/') && !before.ends_with("//") {
        return false;
    }
    !matches!(
        &text[start..end],
        "0.0.0.0" | "127.0.0.1" | "255.255.255.255"
    )
}

/// `java.io.File` style identifiers: the match is followed by `.` + letter.
fn continues_dotted(text: &str, end: usize) -> bool {
    let mut rest = text[end..].chars();
    rest.next() == Some('.') && rest.next().is_some_and(|c| c.is_ascii_alphanumeric())
}
