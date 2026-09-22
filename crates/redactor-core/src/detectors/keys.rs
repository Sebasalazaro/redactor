//! Key-aware detection for structured bodies: JSON (including JSON escaped
//! inside HAR strings), HAR / Postman `name`-`value` pairs, query strings and
//! form bodies. The key decides how the value is masked.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::{Scan, http};
use crate::finding::Category;
use crate::redactor::normalize_key;

/// `"key": "` — the opening of a JSON string value. Quotes may be escaped
/// (`\"key\": \"`) when the JSON sits inside another JSON string.
static JSON_STRING_PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\\?"(?P<key>[A-Za-z0-9_.$@-]{1,64})\\?"[ \t]*:[ \t]*(?P<quote>\\?")"#).unwrap()
});

/// `"key": 1042`.
static JSON_NUMBER_PAIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\\?"(?P<key>[A-Za-z0-9_.$@-]{1,64})\\?"[ \t]*:[ \t]*(?P<num>\d{2,})\b"#).unwrap()
});

/// HAR headers / cookies and Postman params: `{"name": "X", "value": "..."}`.
static NAME_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\\?"(?:name|key)\\?"\s*:\s*\\?"(?P<name>[^"\\\r\n]{1,128})\\?"\s*,\s*\\?"value\\?"\s*:\s*(?P<quote>\\?")"#,
    )
    .unwrap()
});

/// What follows the `name` of a name-value pair; used to skip it in the
/// plain JSON pass.
static VALUE_FOLLOWS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\\?"\s*,\s*\\?"value\\?"\s*:"#).unwrap());

/// `{"name": "Firefox", "version": "131.0"}`: a product, not a person.
static VERSION_FOLLOWS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\\?"\s*,\s*\\?"version\\?"\s*:"#).unwrap());

static COOKIES_ARRAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\\?"cookies\\?"\s*:\s*\["#).unwrap());

/// `key=value` in query strings, form bodies and logs.
static PARAM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)(?:^|[?&;\s])(?P<key>[A-Za-z0-9_.\[\]-]{1,64})=(?P<value>[^&\s"'#;<>\\]*)"#)
        .unwrap()
});

/// Keys whose value is a secret, compared after [`normalize_key`].
const SECRET_KEYS: &[&str] = &[
    "auth",
    "cookie",
    "cvc",
    "cvv",
    "otp",
    "pass",
    "pin",
    "pwd",
    "session",
    "sid",
    "sig",
    "signature",
    "ssn",
];

/// Substrings that make a key secret (`newPassword`, `client_secret`, ...).
const SECRET_KEY_PARTS: &[&str] = &[
    "apikey",
    "credential",
    "passwd",
    "password",
    "privatekey",
    "secret",
    "sessionid",
    "token",
];

/// Suffixes that describe a secret without being one (`token_type`, `expires_in`).
const METADATA_SUFFIXES: &[&str] = &[
    "expires",
    "expiresin",
    "length",
    "ttl",
    "type",
    "uri",
    "url",
];

/// Secret only in query strings and form bodies (OAuth `code`).
const SECRET_PARAMS: &[&str] = &["code"];

/// Keys whose value is personal data.
const PII_KEYS: &[&str] = &[
    "accountholder",
    "accountname",
    "address",
    "apellido",
    "birthdate",
    "cardholder",
    "cardholdername",
    "cedula",
    "cellphone",
    "celular",
    "contactname",
    "correo",
    "customername",
    "direccion",
    "displayname",
    "dni",
    "dob",
    "document",
    "documentnumber",
    "email",
    "familyname",
    "firstname",
    "fullname",
    "givenname",
    "holdername",
    "lastname",
    "login",
    "mail",
    "mobile",
    "name",
    "nationalid",
    "nickname",
    "nombre",
    "ownername",
    "passport",
    "phone",
    "phonenumber",
    "street",
    "surname",
    "telefono",
    "username",
    "usuario",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyClass {
    Secret,
    Pii,
    Id,
}

pub(super) fn detect(scan: &mut Scan) {
    let text = scan.text;

    for caps in JSON_STRING_PAIR.captures_iter(text) {
        let key = &caps["key"];
        let quote = caps.name("quote").unwrap();
        let escaped = quote.as_str().starts_with('\\');
        let Some(end) = string_end(text, quote.end(), escaped) else {
            continue;
        };
        let rest = &text[end..];
        if matches!(key, "name" | "key") && VALUE_FOLLOWS.is_match(rest)
            || key == "name" && VERSION_FOLLOWS.is_match(rest)
        {
            continue;
        }
        if scan.r.is_sensitive_header(key) {
            http::header_value(scan, &key.to_ascii_lowercase(), quote.end(), end);
        } else if let Some(class) = classify(scan, key, false) {
            value(scan, class, quote.end(), end);
        }
    }

    for caps in JSON_NUMBER_PAIR.captures_iter(text) {
        if classify(scan, &caps["key"], false) == Some(KeyClass::Id) {
            let num = caps.name("num").unwrap();
            value(scan, KeyClass::Id, num.start(), num.end());
        }
    }

    let cookie_arrays = cookie_arrays(text);
    for caps in NAME_VALUE.captures_iter(text) {
        let name = &caps["name"];
        let quote = caps.name("quote").unwrap();
        let escaped = quote.as_str().starts_with('\\');
        let Some(end) = string_end(text, quote.end(), escaped) else {
            continue;
        };
        let start = quote.end();
        if cookie_arrays.iter().any(|r| r.contains(&start)) {
            if start < end {
                scan.token(start, end);
            }
        } else if scan.r.is_sensitive_header(name) {
            http::header_value(scan, &name.to_ascii_lowercase(), start, end);
        } else if let Some(class) = classify(scan, name, true) {
            value(scan, class, start, end);
        }
    }

    for caps in PARAM.captures_iter(text) {
        let v = caps.name("value").unwrap();
        if let Some(class) = classify(scan, &caps["key"], true) {
            value(scan, class, v.start(), v.end());
        }
    }
}

fn value(scan: &mut Scan, class: KeyClass, start: usize, end: usize) {
    if start >= end {
        return;
    }
    let text = scan.text;
    let v = &text[start..end];
    match class {
        KeyClass::Secret => scan.token(start, end),
        KeyClass::Pii => scan.push(start, end, Category::Pii, scan.r.mask_pii(v)),
        KeyClass::Id => scan.push(start, end, Category::Id, scan.r.mask_id(v)),
    }
}

fn classify(scan: &Scan, key: &str, is_param: bool) -> Option<KeyClass> {
    let norm = normalize_key(key);
    if norm.is_empty() {
        return None;
    }
    let is_metadata = METADATA_SUFFIXES.iter().any(|s| norm.ends_with(s));
    if scan.r.sensitive_keys.contains(&norm)
        || SECRET_KEYS.contains(&norm.as_str())
        || (is_param && SECRET_PARAMS.contains(&norm.as_str()))
        || (!is_metadata && SECRET_KEY_PARTS.iter().any(|p| norm.contains(p)))
    {
        return Some(KeyClass::Secret);
    }
    if is_id_key(key) {
        return Some(KeyClass::Id);
    }
    if PII_KEYS.contains(&norm.as_str()) {
        return Some(KeyClass::Pii);
    }
    None
}

/// `id`, `user_id`, `user-id`, `userId`, `userID`, `uuid`, `guid`. Plain
/// words ending in "id" (`paid`, `valid`) are not identifiers.
fn is_id_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    matches!(lower.as_str(), "id" | "uid" | "uuid" | "guid")
        || lower.ends_with("_id")
        || lower.ends_with("-id")
        || lower.ends_with(".id")
        || (key.len() > 2 && (key.ends_with("Id") || key.ends_with("ID")))
}

/// Byte index of the closing quote of a JSON string whose content starts at
/// `from`. With `escaped`, the closing quote is `\"`.
fn string_end(text: &str, from: usize, escaped: bool) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => return None,
            b'\\' if escaped => match bytes.get(i + 1) {
                Some(b'"') => return Some(i),
                Some(b'\\') => i += 2,
                _ => i += 1,
            },
            b'\\' => i += 2,
            b'"' if !escaped => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Ranges covered by `"cookies": [ ... ]` arrays (HAR).
fn cookie_arrays(text: &str) -> Vec<Range<usize>> {
    COOKIES_ARRAY
        .find_iter(text)
        .filter_map(|m| {
            let bytes = text.as_bytes();
            let (mut depth, mut in_string, mut i) = (1usize, false, m.end());
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' => i += 1,
                    b'"' => in_string = !in_string,
                    b'[' if !in_string => depth += 1,
                    b']' if !in_string => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(m.end()..i);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_lists_are_consistent() {
        for list in [SECRET_KEYS, PII_KEYS, SECRET_KEY_PARTS, METADATA_SUFFIXES] {
            assert!(list.iter().all(|k| normalize_key(k) == *k), "{list:?}");
        }
    }

    #[test]
    fn recognizes_id_keys() {
        for key in ["id", "user_id", "userId", "accountID", "order-id", "uuid"] {
            assert!(is_id_key(key), "{key}");
        }
        for key in ["paid", "valid", "Id2", "idempotency"] {
            assert!(!is_id_key(key), "{key}");
        }
    }

    #[test]
    fn finds_string_ends() {
        assert_eq!(string_end(r#"abc\"d" rest"#, 0, false), Some(6));
        assert_eq!(string_end(r#"ab\\cd\" rest"#, 0, true), Some(6));
        assert_eq!(string_end("abc\n\"", 0, false), None);
    }
}
