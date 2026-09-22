//! Input format detection and normalization.

use std::borrow::Cow;
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use regex::{Captures, Regex};
use serde::Serialize;

/// What the input looks like. Detection is a best effort: every format goes
/// through the same detectors, this only drives normalization and reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFormat {
    HttpRequest,
    HttpResponse,
    Curl,
    Fetch,
    Har,
    BurpXml,
    Postman,
    Text,
}

impl InputFormat {
    pub fn detect(input: &str) -> Self {
        let head = input.trim_start();
        let first_line = head.lines().next().unwrap_or_default();

        if head.starts_with('<') && (head.contains("burpVersion") || head.contains("<item>")) {
            return Self::BurpXml;
        }
        if head.starts_with('{') {
            if head.contains("\"_postman_id\"") || head.contains("schema.getpostman.com") {
                return Self::Postman;
            }
            if head.contains("\"log\"") && head.contains("\"entries\"") {
                return Self::Har;
            }
        }
        if head.starts_with("curl ") {
            return Self::Curl;
        }
        if head.starts_with("fetch(") || head.starts_with("await fetch(") {
            return Self::Fetch;
        }
        if REQUEST_LINE.is_match(first_line) {
            return Self::HttpRequest;
        }
        if first_line.starts_with("HTTP/") {
            return Self::HttpResponse;
        }
        Self::Text
    }
}

static REQUEST_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z]{3,10} \S+ HTTP/\d(?:\.\d)?$").unwrap());

static BURP_BASE64: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"base64="true"><!\[CDATA\[(?P<body>[A-Za-z0-9+/=\s]*)\]\]>"#).unwrap()
});

/// Makes the input readable for the detectors. Burp exports store requests
/// and responses as base64; they are decoded in place (and flagged as
/// `base64="false"`) so the LLM receives readable, redacted traffic.
pub fn normalize(input: &str, format: InputFormat) -> Cow<'_, str> {
    if format != InputFormat::BurpXml {
        return Cow::Borrowed(input);
    }
    BURP_BASE64.replace_all(input, |caps: &Captures| {
        let body: String = caps["body"].split_whitespace().collect();
        match STANDARD.decode(body) {
            Ok(bytes) => format!(
                r#"base64="false"><![CDATA[{}]]>"#,
                String::from_utf8_lossy(&bytes)
            ),
            Err(_) => caps[0].to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        assert_eq!(
            InputFormat::detect("GET /a HTTP/1.1\nHost: x"),
            InputFormat::HttpRequest
        );
        assert_eq!(
            InputFormat::detect("HTTP/2 200\n"),
            InputFormat::HttpResponse
        );
        assert_eq!(
            InputFormat::detect("curl -X POST https://x"),
            InputFormat::Curl
        );
        assert_eq!(
            InputFormat::detect("fetch(\"https://x\")"),
            InputFormat::Fetch
        );
        assert_eq!(
            InputFormat::detect(r#"{"log": {"entries": []}}"#),
            InputFormat::Har
        );
        assert_eq!(
            InputFormat::detect(r#"{"info": {"_postman_id": "1"}}"#),
            InputFormat::Postman
        );
        assert_eq!(
            InputFormat::detect(r#"<?xml version="1.0"?><items burpVersion="2026.1">"#),
            InputFormat::BurpXml
        );
        assert_eq!(InputFormat::detect("nmap scan report"), InputFormat::Text);
    }

    #[test]
    fn decodes_burp_bodies() {
        let body = STANDARD.encode("GET / HTTP/1.1\r\nHost: a\r\n\r\n");
        let xml = format!(
            r#"<items burpVersion="1"><request base64="true"><![CDATA[{body}]]></request>"#
        );
        let normalized = normalize(&xml, InputFormat::BurpXml);
        assert!(normalized.contains(r#"base64="false"><![CDATA[GET / HTTP/1.1"#));
    }
}
