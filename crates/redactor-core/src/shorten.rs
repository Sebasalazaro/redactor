//! Shortening long, repetitive JSON lists before sharing traffic.
//!
//! An API response often returns hundreds of entries with the same shape.
//! An LLM needs two of them to understand the structure; the rest only cost
//! tokens and add values to redact. Lists of more than [`MIN_ITEMS`] items
//! whose entries share their shape keep the first [`KEEP`] and end with a
//! note saying how many were left out:
//!
//! ```json
//! "users": [{"id": 1, "name": "…"}, {"id": 2, "name": "…"}, "… 48 more items like these"]
//! ```
//!
//! The result is still valid JSON. Only JSON documents that change are
//! rewritten (keeping their indentation and key order); everything around
//! them, such as HTTP headers, is left as it was. Exports meant to be kept
//! whole (HAR, Burp, Postman) are never shortened.

use serde_json::Value;
use serde_json::ser::{PrettyFormatter, Serializer};

use crate::format::InputFormat;

/// Entries kept from a shortened list.
pub const KEEP: usize = 2;
/// Lists with more entries than this are shortened.
pub const MIN_ITEMS: usize = 3;

/// A shortened text and what was left out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortened {
    pub text: String,
    /// Lists that were shortened.
    pub lists: usize,
    /// Entries left out across them.
    pub removed: usize,
}

/// Shortens the long, repetitive lists of every JSON document in `text`.
/// Returns `None` when there was nothing to shorten.
pub fn shorten_lists(text: &str) -> Option<Shortened> {
    if matches!(
        InputFormat::detect(text),
        InputFormat::Har | InputFormat::BurpXml | InputFormat::Postman
    ) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let (mut lists, mut removed) = (0, 0);
    let mut copied = 0;
    let mut i = 0;
    while i < text.len() {
        let Some(start) = next_document_start(text, i) else {
            break;
        };
        let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
        let parsed = stream.next();
        let end = start + stream.byte_offset();
        match parsed {
            Some(Ok(mut value)) if value.is_array() || value.is_object() => {
                let mut stats = (0, 0);
                shorten(&mut value, &mut stats);
                if stats.0 > 0 {
                    out.push_str(&text[copied..start]);
                    out.push_str(&serialize(&value, &text[start..end]));
                    copied = end;
                    lists += stats.0;
                    removed += stats.1;
                }
                i = end.max(start + 1);
            }
            _ => i = start + 1,
        }
    }
    if lists == 0 {
        return None;
    }
    out.push_str(&text[copied..]);
    Some(Shortened {
        text: out,
        lists,
        removed,
    })
}

/// The next `{` or `[` that opens a line (after optional spaces): where a
/// body or a pasted JSON document starts. Brackets in the middle of a line
/// (`--data '{…}'`, a header value) are not tried.
fn next_document_start(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        let line_start = i == 0 || bytes[i - 1] == b'\n';
        if line_start {
            let mut j = i;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                j += 1;
            }
            if j < bytes.len() && matches!(bytes[j], b'{' | b'[') {
                return Some(j);
            }
        }
        i += 1;
    }
    None
}

/// Shortens in place; counts `(lists, removed entries)`.
fn shorten(value: &mut Value, stats: &mut (usize, usize)) {
    match value {
        Value::Array(items) => {
            let long = items.len() > MIN_ITEMS && same_shape(items);
            if long {
                let left_out = items.len() - KEEP;
                items.truncate(KEEP);
                stats.0 += 1;
                stats.1 += left_out;
                for item in items.iter_mut() {
                    shorten(item, stats);
                }
                items.push(Value::String(format!("… {left_out} more items like these")));
            } else {
                for item in items.iter_mut() {
                    shorten(item, stats);
                }
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                shorten(v, stats);
            }
        }
        _ => {}
    }
}

/// Objects with the same keys (values may differ), or plain values of one
/// type: strings, numbers or booleans.
fn same_shape(items: &[Value]) -> bool {
    fn keys(v: &Value) -> Option<Vec<&str>> {
        v.as_object().map(|m| {
            let mut k: Vec<&str> = m.keys().map(String::as_str).collect();
            k.sort_unstable();
            k
        })
    }
    match &items[0] {
        Value::Object(_) => {
            let first = keys(&items[0]);
            items.iter().all(|v| keys(v) == first)
        }
        Value::String(_) => items.iter().all(Value::is_string),
        Value::Number(_) => items.iter().all(Value::is_number),
        Value::Bool(_) => items.iter().all(Value::is_boolean),
        _ => false,
    }
}

/// Writes `value` the way `original` was written: on one line, or indented
/// with the same unit (two spaces, four spaces, a tab).
fn serialize(value: &Value, original: &str) -> String {
    if !original.trim().contains('\n') {
        return value.to_string();
    }
    let indent = original
        .lines()
        .skip(1)
        .find_map(|l| {
            let n = l.len() - l.trim_start().len();
            (n > 0).then(|| l[..n].to_string())
        })
        .unwrap_or_else(|| "  ".into());
    let mut out = Vec::new();
    let mut ser =
        Serializer::with_formatter(&mut out, PrettyFormatter::with_indent(indent.as_bytes()));
    serde::Serialize::serialize(value, &mut ser).expect("a Value always serializes");
    let mut text = String::from_utf8(out).expect("serde_json writes UTF-8");
    if original.contains("\r\n") {
        text = text.replace('\n', "\r\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn users(n: usize) -> String {
        let items: Vec<String> = (0..n)
            .map(|i| {
                format!(r#"{{"id": {i}, "name": "user{i}", "tags": ["a", "b", "c", "d", "e"]}}"#)
            })
            .collect();
        format!("[{}]", items.join(", "))
    }

    #[test]
    fn keeps_two_entries_and_counts_the_rest() {
        let s = shorten_lists(&users(50)).unwrap();
        let v: Value = serde_json::from_str(&s.text).unwrap();
        let list = v.as_array().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[2], "… 48 more items like these");
        // The kept entries' own long lists are shortened too.
        assert_eq!(
            list[0]["tags"],
            serde_json::json!(["a", "b", "… 3 more items like these"])
        );
        assert_eq!((s.lists, s.removed), (3, 54));
    }

    #[test]
    fn leaves_short_or_mixed_lists_alone() {
        assert_eq!(shorten_lists(r#"[{"a": 1}, {"a": 2}, {"a": 3}]"#), None);
        assert_eq!(
            shorten_lists(r#"[{"a": 1}, {"b": 2}, {"a": 3}, {"a": 4}]"#),
            None
        );
        assert_eq!(shorten_lists(r#"[1, "two", 3, 4, 5]"#), None);
        assert_eq!(shorten_lists("plain text [1, 2, 3, 4, 5]"), None);
    }

    #[test]
    fn rewrites_only_the_body_of_an_http_response() {
        let body = "{\r\n    \"total\": 5,\r\n    \"items\": [1, 2, 3, 4, 5]\r\n}";
        let response =
            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{body}\r\n");
        let s = shorten_lists(&response).unwrap();
        assert!(
            s.text.starts_with(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\r\n    \"total\": 5,"
            ),
            "{}",
            s.text
        );
        assert!(
            s.text.contains("\"… 3 more items like these\""),
            "{}",
            s.text
        );
        assert!(s.text.ends_with("}\r\n"), "{:?}", s.text);
    }

    #[test]
    fn handles_a_request_and_its_response_pasted_together() {
        let text = format!(
            "POST /api/search HTTP/1.1\nHost: api.example.com\n\n{{\"q\": \"x\"}}\n\nHTTP/1.1 200 OK\n\n{}\n",
            users(10)
        );
        let s = shorten_lists(&text).unwrap();
        assert!(s.text.contains("{\"q\": \"x\"}"), "untouched request body");
        assert!(s.text.contains("8 more items like these"));
    }

    #[test]
    fn kept_entries_are_still_redacted() {
        let r = crate::Redactor::new(&crate::Config {
            client: vec!["Globex".into()],
            ..Default::default()
        });
        let items: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"email": "user{i}@globex.example", "token": "a7f3e9c1d5b2084f6e1a9c3d7b5f2e{i:02}"}}"#))
            .collect();
        let response = format!("HTTP/1.1 200 OK\n\n{{\"data\": [{}]}}\n", items.join(","));
        let short = shorten_lists(&response).unwrap();
        let out = r.redact(&short.text).output;
        assert!(out.contains("… 18 more items like these"), "{out}");
        assert!(
            !out.contains("user0@") && !out.to_lowercase().contains("globex"),
            "{out}"
        );
        assert!(!out.contains("a7f3e9c1d5b2084f6e1a9c3d7b5f2e00"), "{out}");
        assert!(!out.contains("user5@"), "left-out entries are gone");
    }

    #[test]
    fn never_shortens_exports() {
        let har = format!(
            r#"{{"log": {{"version": "1.2", "entries": {}}}}}"#,
            users(10)
        );
        assert_eq!(shorten_lists(&har), None);
    }
}
