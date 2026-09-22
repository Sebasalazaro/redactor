//! HTTP headers, cookies and authorization schemes, wherever they appear:
//! raw requests, `curl -H '...'`, logs.

use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use regex::Regex;

use super::Scan;
use crate::finding::Category;

/// `Name: value`, optionally opened by a quote (`-H 'Cookie: a=b'`).
static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?P<quote>['"]?)(?P<name>[A-Za-z][A-Za-z0-9-]*)[ \t]*:[ \t]*"#).unwrap()
});

/// `curl -u user:pass`.
static CURL_USER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s)(?:-u|--user)[ \t]+(?:'(?P<a>[^']*)'|"(?P<b>[^"]*)"|(?P<c>[^\s'"]+))"#)
        .unwrap()
});

/// `curl -b 'a=1; b=2'`.
static CURL_COOKIE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|\s)(?:-b|--cookie)[ \t]+(?:'(?P<a>[^']*)'|"(?P<b>[^"]*)"|(?P<c>[^\s'"]+))"#)
        .unwrap()
});

/// `key=value` pairs of non-bearer authorization schemes (Digest, AWS SigV4...).
static AUTH_PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?P<key>[A-Za-z0-9_-]+)=(?:"(?P<quoted>[^"]*)"|(?P<bare>[^,\s]*))"#).unwrap()
});

/// Authorization parameters that describe the request rather than prove identity.
const PUBLIC_AUTH_PARAMS: &[&str] = &["algorithm", "nc", "qop", "realm", "signedheaders", "uri"];

pub(super) fn detect(scan: &mut Scan) {
    let text = scan.text;

    for caps in HEADER.captures_iter(text) {
        let name = &caps["name"];
        if !scan.r.is_sensitive_header(name) {
            continue;
        }
        let start = caps.get(0).unwrap().end();
        let mut end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        if let Some(quote) = caps["quote"].chars().next() {
            end = text[start..end].find(quote).map_or(end, |i| start + i);
        }
        let end = start + text[start..end].trim_end().len();
        header_value(scan, &name.to_ascii_lowercase(), start, end);
    }

    for caps in CURL_USER.captures_iter(text) {
        let value = caps
            .name("a")
            .or(caps.name("b"))
            .or(caps.name("c"))
            .unwrap();
        credentials(scan, value.start(), value.end());
    }

    for caps in CURL_COOKIE.captures_iter(text) {
        let value = caps
            .name("a")
            .or(caps.name("b"))
            .or(caps.name("c"))
            .unwrap();
        cookies(scan, value.start(), value.end(), false);
    }
}

/// Redacts the value of a sensitive header located at `start..end`.
pub(super) fn header_value(scan: &mut Scan, name: &str, start: usize, end: usize) {
    if start >= end {
        return;
    }
    match name {
        "cookie" => cookies(scan, start, end, false),
        "set-cookie" => cookies(scan, start, end, true),
        "authorization" | "proxy-authorization" => authorization(scan, start, end),
        _ => scan.token(start, end),
    }
}

/// `a=1; b=2`. For `Set-Cookie` only the first pair is a value; the rest are
/// attributes (`Path`, `Domain`, ...), which stay readable.
fn cookies(scan: &mut Scan, start: usize, end: usize, only_first: bool) {
    let text = scan.text;
    let mut offset = start;
    for part in text[start..end].split(';') {
        let part_start = offset;
        offset += part.len() + 1;
        if let Some(eq) = part.find('=') {
            let value = part[eq + 1..].trim_end();
            let value_start = part_start + eq + 1;
            let value_start = value_start + (value.len() - value.trim_start().len());
            let value_end = part_start + eq + 1 + value.len();
            if value_start < value_end {
                scan.token(value_start, value_end);
            }
        }
        if only_first {
            break;
        }
    }
}

fn authorization(scan: &mut Scan, start: usize, end: usize) {
    let text = scan.text;
    let value = &text[start..end];
    let Some(space) = value.find(char::is_whitespace) else {
        return scan.token(start, end);
    };
    let rest_start = start + space + (value[space..].len() - value[space..].trim_start().len());
    if rest_start >= end {
        return;
    }
    match value[..space].to_ascii_lowercase().as_str() {
        "basic" => basic(scan, rest_start, end),
        "bearer" | "token" | "jwt" | "sso-key" => scan.token(rest_start, end),
        _ => auth_params(scan, rest_start, end),
    }
}

/// `Basic base64(user:pass)` becomes `Basic [basic-auth us**:Pa…[len=12]]`.
fn basic(scan: &mut Scan, start: usize, end: usize) {
    let decoded = STANDARD
        .decode(&scan.text[start..end])
        .ok()
        .and_then(|b| String::from_utf8(b).ok());
    match decoded {
        Some(pair) if pair.contains(':') => {
            let (user, pass) = pair.split_once(':').unwrap();
            let replacement = format!(
                "[basic-auth {}:{}]",
                scan.r.mask_pii(user),
                scan.r.mask_secret(pass)
            );
            scan.push(start, end, Category::Credential, replacement);
        }
        _ => scan.token(start, end),
    }
}

fn auth_params(scan: &mut Scan, start: usize, end: usize) {
    let text = scan.text;
    let mut found = false;
    for caps in AUTH_PARAM.captures_iter(&text[start..end]) {
        found = true;
        let key = caps["key"].to_ascii_lowercase();
        if PUBLIC_AUTH_PARAMS.contains(&key.as_str()) {
            continue;
        }
        let value = caps.name("quoted").or(caps.name("bare")).unwrap();
        if !value.is_empty() {
            scan.token(start + value.start(), start + value.end());
        }
    }
    if !found {
        scan.token(start, end);
    }
}

/// `user:pass` from `curl -u`.
fn credentials(scan: &mut Scan, start: usize, end: usize) {
    let text = scan.text;
    let value = &text[start..end];
    let replacement = match value.split_once(':') {
        Some((user, pass)) => format!("{}:{}", scan.r.mask_pii(user), scan.r.mask_secret(pass)),
        None => scan.r.mask_pii(value),
    };
    scan.push(start, end, Category::Credential, replacement);
}
