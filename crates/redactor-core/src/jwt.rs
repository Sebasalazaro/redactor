//! JWT decoding with claim-aware redaction.
//!
//! A JWT is far more useful to an LLM decoded than opaque: `alg`, roles,
//! scopes and expiry drive most token attacks. The token is rendered as
//!
//! ```text
//! [JWT header{alg=RS256, typ=JWT} payload{sub=4f2a****, name=Ju** Pé***, roles=[admin]} sig=…[len=342]]
//! ```
//!
//! using `key=value` notation instead of JSON so the result can sit inside
//! quoted strings (JSON bodies, curl arguments) without breaking them. The
//! signature is never shown.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;

use crate::redactor::{Redactor, normalize_key};

/// Claims kept verbatim (after client-name replacement): they describe the
/// token, not the person or the tenant.
const DESCRIPTIVE_CLAIMS: &[&str] = &[
    "acr",
    "alg",
    "amr",
    "authtime",
    "cty",
    "enc",
    "exp",
    "grant",
    "granttype",
    "groups",
    "gty",
    "iat",
    "locale",
    "nbf",
    "permissions",
    "role",
    "roles",
    "scope",
    "scp",
    "tokenuse",
    "typ",
    "ver",
    "version",
    "zoneinfo",
];

/// Identifier claims: hide the tail like any other id.
const ID_CLAIMS: &[&str] = &[
    "appid", "athash", "azp", "chash", "clientid", "jti", "kid", "nonce", "oid", "sid", "sub",
    "tid", "uid", "userid",
];

/// Identity claims: keep the head of each word.
const PII_CLAIMS: &[&str] = &[
    "aud",
    "email",
    "familyname",
    "givenname",
    "iss",
    "name",
    "nickname",
    "preferredusername",
    "uniquename",
    "upn",
    "username",
];

/// Cheap shape check before trying to decode.
pub(crate) fn looks_like_jwt(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(h), Some(p), Some(_), None) if h.starts_with("eyJ") && p.starts_with("eyJ")
    )
}

/// Decodes and redacts a JWT. Returns `None` if it does not decode, so the
/// caller can fall back to treating it as an opaque secret.
pub(crate) fn render(r: &Redactor, token: &str, depth: usize) -> Option<String> {
    if !looks_like_jwt(token) {
        return None;
    }
    let mut parts = token.split('.');
    let header = decode_segment(parts.next()?)?;
    let payload = decode_segment(parts.next()?)?;
    let signature = parts.next().unwrap_or_default();

    let signature = if signature.is_empty() {
        "<none>".to_string()
    } else {
        format!("…[len={}]", signature.len())
    };
    Some(format!(
        "[JWT header{} payload{} sig={}]",
        render_value(&sanitize(r, None, header, depth)),
        render_value(&sanitize(r, None, payload, depth)),
        signature
    ))
}

fn decode_segment(segment: &str) -> Option<Value> {
    let bytes = URL_SAFE_NO_PAD.decode(segment.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn sanitize(r: &Redactor, key: Option<&str>, value: Value, depth: usize) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let v = sanitize(r, Some(&k), v, depth);
                    (k, v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|v| sanitize(r, key, v, depth))
                .collect(),
        ),
        Value::String(s) => Value::String(sanitize_str(r, key, &s, depth)),
        Value::Number(n) if key.is_some_and(|k| claim_in(k, ID_CLAIMS)) => {
            Value::String(r.mask_id(&n.to_string()))
        }
        other => other,
    }
}

fn sanitize_str(r: &Redactor, key: Option<&str>, value: &str, depth: usize) -> String {
    let Some(key) = key else {
        return r.redact_nested(value, depth + 1);
    };
    if claim_in(key, DESCRIPTIVE_CLAIMS) {
        return r
            .dict
            .replace_clients(value)
            .unwrap_or_else(|| value.to_string());
    }
    // Let the full engine handle hosts, emails, uuids and nested tokens first;
    // fall back to partial masking for identity claims it did not touch.
    let redacted = r.redact_nested(value, depth + 1);
    if redacted != value {
        redacted
    } else if claim_in(key, ID_CLAIMS) {
        r.mask_id(value)
    } else if claim_in(key, PII_CLAIMS) {
        r.mask_pii(value)
    } else {
        redacted
    }
}

fn claim_in(key: &str, list: &[&str]) -> bool {
    list.binary_search(&normalize_key(key).as_str()).is_ok()
}

/// `{a=1, b=[x, y]}`: readable, and free of quotes.
fn render_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let fields: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{k}={}", render_value(v)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(render_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn token(header: &str, payload: &str, sig: &str) -> String {
        format!(
            "{}.{}.{sig}",
            URL_SAFE_NO_PAD.encode(header),
            URL_SAFE_NO_PAD.encode(payload)
        )
    }

    #[test]
    fn claim_lists_are_sorted() {
        for list in [DESCRIPTIVE_CLAIMS, ID_CLAIMS, PII_CLAIMS] {
            assert!(list.windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn renders_redacted_claims() {
        let r = Redactor::new(&Config {
            client: vec!["Globex Bank".into(), "Globex".into()],
            ..Default::default()
        });
        let jwt = token(
            r#"{"alg":"RS256","typ":"JWT"}"#,
            r#"{"iss":"https://auth.globexbank.example/realms/retail","sub":"8812734","name":"Juan Pérez","roles":["globex-admin","auditor"],"exp":1790000000}"#,
            "c2lnbmF0dXJlLWJ5dGVz",
        );
        assert_eq!(
            render(&r, &jwt, 0).unwrap(),
            "[JWT header{alg=RS256, typ=JWT} payload{iss=https://auth.[CLIENT].example/realms/retail, \
             sub=88127**, name=Ju** Pé***, roles=[[CLIENT]-admin, auditor], exp=1790000000} sig=…[len=20]]"
        );
    }

    #[test]
    fn flags_unsigned_tokens() {
        let r = Redactor::new(&Config::default());
        let jwt = token(r#"{"alg":"none"}"#, r#"{"role":"user"}"#, "");
        assert_eq!(
            render(&r, &jwt, 0).unwrap(),
            "[JWT header{alg=none} payload{role=user} sig=<none>]"
        );
    }

    #[test]
    fn rejects_non_jwt() {
        let r = Redactor::new(&Config::default());
        assert!(render(&r, "eyJnotbase64.eyJ.x", 0).is_none());
        assert!(!looks_like_jwt("abc.def.ghi"));
    }
}
