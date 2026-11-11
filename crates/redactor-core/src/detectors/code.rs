//! Secrets written into source code, mostly front-end JavaScript served by
//! the target: HMAC and signing keys, `atob`/Base64 decoded constants, and
//! variables or properties named like secrets.
//!
//! Three passes, from most to least certain:
//!
//! 1. A string literal passed to a crypto or decoding call
//!    (`hmac.create(Base64.decode("…"))`, `createHmac("sha256", "…")`,
//!    `atob("…")`, `CryptoJS.enc.Utf8.parse("…")`).
//! 2. A literal assigned to an identifier named like a secret
//!    (`appSecret = "…"`, `apiKey: '…'`, `signingKey = "…"`).
//! 3. Any quoted literal that looks random: 24 or more characters from the
//!    Base64 alphabet, with upper case, lower case and digits, high entropy
//!    and characters that keep changing kind. Pure hex is left alone (asset
//!    hashes, build ids), and so are identifiers, paths and URLs.

use std::sync::LazyLock;

use regex::Regex;

use super::Scan;
use crate::redactor::normalize_key;

/// The opening of a call whose literal argument is key material.
static CRYPTO_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:\b(?:hmac\w*|createhmac|importkey|atob|b64decode|base64decode|decodebase64|frombase64|setsecret|setkey)|base64\.(?:decode|parse)|enc\.(?:base64|utf8|hex|latin1)\.parse)(?:\.[\w$]+)*\s*\(",
    )
    .unwrap()
});

/// `name = "`, `name: '`, `name = \``: an identifier followed by a literal.
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[^\w$."'])(?P<key>[A-Za-z_$][\w$]{1,63})\s*(?P<op>:|=)\s*(?P<quote>["'`])"#)
        .unwrap()
});

/// A quoted literal made only of Base64 / URL-safe characters.
static RANDOM_LITERAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"["'`](?P<value>[A-Za-z0-9+/=_-]{24,512})["'`]"#).unwrap());

/// Name parts that make an identifier a secret in code.
const SECRET_NAME_PARTS: &[&str] = &[
    "accesskey",
    "apikey",
    "appkey",
    "appsecret",
    "clientsecret",
    "hmac",
    "password",
    "privatekey",
    "secret",
    "signingkey",
    "signkey",
];

/// Bits per character above which a literal reads as random. Text and
/// identifiers sit well below; random Base64 of this length sits above.
const MIN_ENTROPY: f64 = 4.0;

/// See [`class_changes`].
const MIN_CLASS_CHANGES: f64 = 0.4;

pub(super) fn detect(scan: &mut Scan) {
    let text = scan.text;

    for m in CRYPTO_CALL.find_iter(text) {
        if let Some((start, end)) = call_literal(text, m.end()) {
            scan.token(start, end);
        }
    }

    for caps in ASSIGNMENT.captures_iter(text) {
        // `a == "x"` compares, it does not assign.
        let op = caps.name("op").unwrap();
        if op.as_str() == "=" && text[op.end()..].starts_with('=') {
            continue;
        }
        let quote = caps.name("quote").unwrap();
        let Some(end) = literal_end(text, quote.end(), quote.as_str()) else {
            continue;
        };
        let (start, key) = (quote.end(), normalize_key(&caps["key"]));
        if is_secret_name(&key) || is_key_name(&key, &text[start..end]) {
            scan.token(start, end);
        }
    }

    for caps in RANDOM_LITERAL.captures_iter(text) {
        let value = caps.name("value").unwrap();
        if looks_random(value.as_str()) {
            scan.token(value.start(), value.end());
        }
    }
}

/// The literal argument of a call opened just before `from`: skips nested
/// calls (`Base64.decode(`) and a short leading literal such as the
/// algorithm in `createHmac("sha256", "…")`.
fn call_literal(text: &str, mut from: usize) -> Option<(usize, usize)> {
    static NESTED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*[A-Za-z_$][\w$.]*\s*\(").unwrap());
    static ALGORITHM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^\s*["'][\w-]{1,16}["']\s*,"#).unwrap());

    for _ in 0..4 {
        match NESTED.find(&text[from..]) {
            Some(m) => from += m.end(),
            None => break,
        }
    }
    if let Some(m) = ALGORITHM.find(&text[from..]) {
        from += m.end();
    }
    let rest = &text[from..];
    let open = rest.len() - rest.trim_start().len();
    let quote = rest[open..]
        .chars()
        .next()
        .filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let start = from + open + 1;
    let end = literal_end(text, start, &quote.to_string())?;
    (end - start >= 6).then_some((start, end))
}

/// End of a string literal that opened with `quote` just before `from`.
/// Template literals with `${…}` and multi-line strings are skipped.
fn literal_end(text: &str, from: usize, quote: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let q = quote.as_bytes()[0];
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' | b'\r' => return None,
            b'$' if q == b'`' && bytes.get(i + 1) == Some(&b'{') => return None,
            b if b == q => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn is_secret_name(key: &str) -> bool {
    SECRET_NAME_PARTS.iter().any(|p| key.contains(p))
        && !["type", "length", "url", "name", "label", "field"]
            .iter()
            .any(|s| key.ends_with(s))
}

/// `appKey`, `X-Ca-Key`, `clientKey` with a token-like value: no spaces,
/// eight characters or more, at least one digit. `sortKey: "createdAt"` is
/// not one.
pub(super) fn is_key_name(key: &str, value: &str) -> bool {
    key.len() > 3
        && key.ends_with("key")
        && value.len() >= 8
        && value.bytes().any(|b| b.is_ascii_digit())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=_-.".contains(&b))
}

fn looks_random(value: &str) -> bool {
    let bytes = value.as_bytes();
    let has = |f: fn(&u8) -> bool| bytes.iter().any(f);
    if !(has(u8::is_ascii_uppercase) && has(u8::is_ascii_lowercase) && has(u8::is_ascii_digit)) {
        return false;
    }
    // Subresource integrity and similar labelled hashes.
    if ["sha256-", "sha384-", "sha512-"]
        .iter()
        .any(|p| value.starts_with(p))
    {
        return false;
    }
    entropy(bytes) >= MIN_ENTROPY && class_changes(bytes) >= MIN_CLASS_CHANGES
}

/// Share of neighbouring characters of different kinds (upper case, lower
/// case, digit, symbol). Random text changes kind about every other
/// character; `webpackChunkMyApp2024Build` only between words.
fn class_changes(bytes: &[u8]) -> f64 {
    let kind = |b: &u8| match b {
        b'A'..=b'Z' => 0,
        b'a'..=b'z' => 1,
        b'0'..=b'9' => 2,
        _ => 3,
    };
    let changes = bytes.windows(2).filter(|w| kind(&w[0]) != kind(&w[1])).count();
    changes as f64 / (bytes.len() - 1) as f64
}

/// Shannon entropy in bits per byte.
fn entropy(bytes: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literals_inside_crypto_calls() {
        let text = r#"x.hmac.create(q.Base64.decode("ZmFrZUtleQ=="));h=createHmac("sha256", 'k3y-0042-fake')"#;
        let found: Vec<_> = CRYPTO_CALL
            .find_iter(text)
            .filter_map(|m| call_literal(text, m.end()))
            .map(|(s, e)| &text[s..e])
            .collect();
        assert!(found.contains(&"ZmFrZUtleQ=="));
        assert!(found.contains(&"k3y-0042-fake"));
    }

    #[test]
    fn tells_random_from_words() {
        assert!(looks_random("ZmFrZVNlY3JldEZvclRlc3RzMTIzNDU2Nzg="));
        assert!(looks_random("Qk9yN2xLbUV4YW1wbGVLZXk5"));
        assert!(!looks_random("handleSubmitButtonClick2"));
        assert!(!looks_random("webpackChunkMyApp2024Build"));
        assert!(!looks_random("d41d8cd98f00b204e9800998ecf8427e"));
        assert!(!looks_random("sha384-oqVuAfXRKap7fdgcCY5uykM6"));
        assert!(!looks_random("AAAAAAAAAAAAbbbbbbbbbbbb1111"));
    }

    #[test]
    fn key_names_need_token_values() {
        assert!(is_key_name("xcakey", "918273645"));
        assert!(is_key_name("appkey", "ak_live_0042fake"));
        assert!(!is_key_name("sortkey", "createdAt"));
        assert!(!is_key_name("i18nkey", "home.title"));
        assert!(!is_key_name("key", "12345678"));
    }

    #[test]
    fn literal_ends() {
        assert_eq!(literal_end(r#"abc\"d" x"#, 0, "\""), Some(6));
        assert_eq!(literal_end("a${b}`", 0, "`"), None);
        assert_eq!(literal_end("ab\ncd'", 0, "'"), None);
    }
}
