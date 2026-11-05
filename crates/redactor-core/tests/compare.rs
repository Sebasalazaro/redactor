//! Site map comparison over fictional Burp exports, built here so that
//! planted values are easy to see next to the assertions that hunt for them.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use redactor_core::sitemap::{self, Class, ReportOptions, SiteMap};
use redactor_core::{Config, Redactor};

const SECRETS: &[&str] = &[
    "globexbank",
    "198.51.100.23",
    "a7f3e9c1d5b2084f6e1a9c3d7b5f2e80",
    "88127",
    "99301",
    "maria.gomez",
    "juan.perez",
    "Maria Gomez",
    "Juan Perez",
    "hunter2",
    "TOPSECRETNOTE",
];

struct Hit {
    method: &'static str,
    path: &'static str,
    status: u16,
    body: &'static str,
}

fn hit(method: &'static str, path: &'static str, status: u16, body: &'static str) -> Hit {
    Hit {
        method,
        path,
        status,
        body,
    }
}

fn export(hits: &[Hit]) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE items [\n<!ELEMENT items (item*)>\n]>\n\
         <items burpVersion=\"2026.8\" exportTime=\"Mon Sep 21 15:04:05 COT 2026\">\n",
    );
    for h in hits {
        let request = format!(
            "{} {} HTTP/1.1\r\nHost: api.globexbank.example\r\nCookie: sid=a7f3e9c1d5b2084f6e1a9c3d7b5f2e80\r\n\r\n",
            h.method, h.path
        );
        let response = format!(
            "HTTP/1.1 {}\r\nSet-Cookie: sid=a7f3e9c1d5b2084f6e1a9c3d7b5f2e80\r\nContent-Type: application/json\r\n\r\n{}",
            h.status, h.body
        );
        xml.push_str(&format!(
            "<item>\n<url><![CDATA[https://api.globexbank.example{path}]]></url>\n\
             <host ip=\"198.51.100.23\">api.globexbank.example</host>\n<port>443</port>\n\
             <protocol>https</protocol>\n<method><![CDATA[{method}]]></method>\n\
             <path><![CDATA[{path}]]></path>\n<extension>null</extension>\n\
             <request base64=\"true\"><![CDATA[{request}]]></request>\n<status>{status}</status>\n\
             <responselength>{len}</responselength>\n<mimetype>JSON</mimetype>\n\
             <response base64=\"true\"><![CDATA[{response}]]></response>\n<comment></comment>\n</item>\n",
            path = h.path,
            method = h.method,
            request = STANDARD.encode(request),
            status = h.status,
            len = response.len(),
            response = STANDARD.encode(response),
        ));
    }
    xml.push_str("</items>\n");
    xml
}

fn site_map(hits: &[Hit]) -> SiteMap {
    sitemap::read_map(export(hits).as_bytes()).unwrap()
}

fn redactor() -> Redactor {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let global = Config::load(root.join("config.toml")).unwrap();
    let engagement = Config::load(root.join("engagement.toml")).unwrap();
    Redactor::new(&global.with_engagement(&engagement))
}

fn admin() -> SiteMap {
    site_map(&[
        hit(
            "GET",
            "/api/v2/customers/88127?token=hunter2&page=1",
            200,
            r#"{"id": 88127, "name": "Maria Gomez", "email": "maria.gomez@globexbank.example", "internalNote": "TOPSECRETNOTE"}"#,
        ),
        hit("GET", "/api/v2/status", 200, r#"{"ok": true}"#),
        hit("GET", "/api/v2/reports", 200, r#"{"rows": [1, 2]}"#),
        hit("DELETE", "/api/v2/users/88127", 204, ""),
        hit("GET", "/api/v2/owners", 403, "denied"),
        hit(
            "GET",
            "/api/v2/admin/maria.gomez@globexbank.example",
            200,
            "{}",
        ),
        hit("GET", "/api/v2/admin/audit", 200, r#"{"entries": []}"#),
        hit("GET", "/assets/app.js", 200, "code"),
    ])
}

fn user() -> SiteMap {
    site_map(&[
        hit(
            "GET",
            "/api/v2/customers/99301?page=2&token=hunter2",
            200,
            r#"{"id": 99301, "name": "Juan Perez", "email": "juan.perez@globexbank.example"}"#,
        ),
        hit("GET", "/api/v2/status", 200, r#"{"ok": true}"#),
        hit("GET", "/api/v2/reports", 403, "denied"),
        hit("DELETE", "/api/v2/users/99301", 403, "denied"),
        hit("GET", "/api/v2/owners", 200, r#"{"list": []}"#),
        hit("GET", "/api/v2/profile", 200, r#"{"me": true}"#),
    ])
}

fn class_of<'a>(rows: &'a [sitemap::Row<'a>], label: &str) -> Class {
    rows.iter()
        .find(|r| r.endpoint.label() == label)
        .unwrap_or_else(|| panic!("no endpoint {label}"))
        .class
}

#[test]
fn classifies_endpoints_for_access_control_review() {
    let (high, low) = (admin(), user());
    let rows = sitemap::compare(&high, &low);
    assert_eq!(class_of(&rows, "GET /api/v2/status"), Class::SameContent);
    assert_eq!(
        class_of(&rows, "GET /api/v2/customers/{id}?page&token"),
        Class::DifferentContent
    );
    assert_eq!(class_of(&rows, "GET /api/v2/reports"), Class::Enforced);
    assert_eq!(
        class_of(&rows, "DELETE /api/v2/users/{id}"),
        Class::Enforced
    );
    assert_eq!(class_of(&rows, "GET /api/v2/owners"), Class::Inverted);
    assert_eq!(class_of(&rows, "GET /api/v2/admin/audit"), Class::HighOnly);
    assert_eq!(class_of(&rows, "GET /api/v2/profile"), Class::LowOnly);
    assert_eq!(high.static_items, 1);
}

#[test]
fn report_keeps_the_signal_without_the_values() {
    let report = sitemap::report(
        &admin(),
        &user(),
        &redactor(),
        &ReportOptions::default(),
        &[],
    );
    for secret in SECRETS {
        assert!(!report.contains(secret), "{secret} leaked:\n{report}");
    }
    for expected in [
        "/api/v2/customers/{id}?page&token",
        "/api/v2/admin/{email}",
        "+internalNote",
        "identical body",
        "different objects",
        "replay these with user's session",
        "api.[CLIENT].example",
    ] {
        assert!(report.contains(expected), "no {expected}:\n{report}");
    }
}

#[test]
fn field_names_are_not_mistaken_for_hosts() {
    let high = site_map(&[hit("GET", "/me", 200, r#"{"user": {"id": 1, "info": 2}}"#)]);
    let low = site_map(&[hit("GET", "/me", 200, r#"{"other": "x"}"#)]);
    let report = sitemap::report(&high, &low, &redactor(), &ReportOptions::default(), &[]);
    assert!(report.contains("+user.id"), "{report}");
    assert!(report.contains("+user.info"), "{report}");
    assert!(report.contains("-other"), "{report}");
}

#[test]
fn data_used_as_json_keys_is_hidden() {
    let high = site_map(&[hit(
        "GET",
        "/map",
        200,
        r#"{"maria.gomez@globexbank.example": 1, "88127": {"role": "x"}}"#,
    )]);
    let low = site_map(&[hit("GET", "/map", 200, "{}")]);
    let report = sitemap::report(&high, &low, &redactor(), &ReportOptions::default(), &[]);
    assert!(report.contains("+{key}.role"), "{report}");
    for secret in ["maria.gomez", "88127"] {
        assert!(!report.contains(secret), "{secret} leaked:\n{report}");
    }
}

#[test]
fn unrequested_items_do_not_count_as_seen() {
    let xml = "<items burpVersion=\"1\"><item><host>a.example</host><port>443</port>\
               <protocol>https</protocol><method>GET</method><path>/ghost</path>\
               <status></status><response base64=\"true\"></response></item></items>";
    let map = sitemap::read_map(xml.as_bytes()).unwrap();
    assert_eq!(map.unrequested, 1);
    assert!(map.endpoints.is_empty());
}

#[test]
fn long_sections_are_cut_and_counted() {
    let hits: Vec<Hit> = ["/a", "/b", "/c"]
        .into_iter()
        .map(|path| hit("GET", path, 200, "{}"))
        .collect();
    let options = ReportOptions {
        max_rows: 1,
        ..Default::default()
    };
    let report = sitemap::report(&site_map(&hits), &site_map(&[]), &redactor(), &options, &[]);
    assert!(report.contains("2 more not listed"), "{report}");
}

#[test]
fn samples_are_redacted_and_limited_to_the_filter() {
    let export = export(&[
        hit(
            "GET",
            "/api/v2/customers/88127",
            200,
            r#"{"email": "maria.gomez@globexbank.example", "password": "hunter2"}"#,
        ),
        hit("GET", "/api/v2/status", 200, r#"{"ok": true}"#),
    ]);
    let samples =
        sitemap::read_samples(export.as_bytes(), "admin", "customers/{id}", &redactor()).unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].endpoint, "GET /api/v2/customers/{id}");
    for secret in SECRETS {
        assert!(
            !samples[0].text.contains(secret),
            "{secret} leaked:\n{}",
            samples[0].text
        );
    }
}
