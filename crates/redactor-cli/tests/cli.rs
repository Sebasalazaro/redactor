use std::io::Write;
use std::process::{Command, Stdio};

fn redactor(args: &[&str], stdin: &str) -> (String, String) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let mut child = Command::new(env!("CARGO_BIN_EXE_redactor"))
        .args(["-c", &format!("{root}/config.toml")])
        .args(["-e", &format!("{root}/engagement.toml")])
        .args(args)
        .env("REDACTOR_CONFIG_DIR", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

const REQUEST: &str =
    "GET /users/88127 HTTP/1.1\nHost: api.globexbank.example\nCookie: sid=a7f3e9c1d5b2084f\n";

#[test]
fn redacts_stdin() {
    let (stdout, stderr) = redactor(&[], REQUEST);
    assert_eq!(
        stdout,
        "GET /users/8812* HTTP/1.1\nHost: api.[CLIENT].example\nCookie: sid=a7f3…[len=16]\n"
    );
    assert!(stderr.is_empty());
}

#[test]
fn reports_to_stderr() {
    let (_, stderr) = redactor(&["--report"], REQUEST);
    assert!(stderr.contains("3 values redacted"), "{stderr}");
}

#[test]
fn json_never_contains_originals() {
    let (stdout, _) = redactor(&["--json"], REQUEST);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["format"], "http_request");
    assert_eq!(json["findings"].as_array().unwrap().len(), 3);
    assert!(!stdout.contains("a7f3e9c1d5b2084f") && !stdout.contains("globexbank"));
}

#[test]
fn remembers_and_restores_through_the_vault() {
    let dir = std::env::temp_dir().join(format!("redactor-cli-vault-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let run = |args: &[&str], stdin: &str| {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
        let mut child = Command::new(env!("CARGO_BIN_EXE_redactor"))
            .args(["-c", &format!("{root}/config.toml")])
            .args(args)
            .env("REDACTOR_CONFIG_DIR", &dir)
            .env("REDACTOR_VAULT_KEY", "11".repeat(32))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8(out.stdout).unwrap(),
            String::from_utf8(out.stderr).unwrap(),
        )
    };

    let (redacted, _) = run(&["--remember"], REQUEST);
    let answer = format!("Try this:\n{redacted}");
    let (restored, stderr) = run(&["--restore", "--report"], &answer);
    assert_eq!(restored, format!("Try this:\n{REQUEST}"));
    assert!(
        stderr.contains("3 values restored, 0 ambiguous"),
        "{stderr}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

fn burp_item(path: &str, status: u16, body: &str) -> String {
    format!(
        "<item><host>api.globexbank.example</host><port>443</port><protocol>https</protocol>\
         <method>GET</method><path><![CDATA[{path}]]></path><status>{status}</status>\
         <mimetype>JSON</mimetype><response base64=\"false\"><![CDATA[HTTP/1.1 {status}\n\
         Set-Cookie: sid=a7f3e9c1d5b2084f\n\n{body}]]></response></item>"
    )
}

#[test]
fn compares_two_site_map_exports() {
    let dir = std::env::temp_dir().join(format!("redactor-cli-compare-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, items: &[String]| {
        let path = dir.join(name);
        let xml = format!("<items burpVersion=\"1\">{}</items>", items.concat());
        std::fs::write(&path, xml).unwrap();
        path
    };
    let high = write(
        "high.xml",
        &[
            burp_item(
                "/users/88127",
                200,
                r#"{"email": "maria.gomez@globexbank.example"}"#,
            ),
            burp_item("/admin/audit", 200, "{}"),
        ],
    );
    let low = write("low.xml", &[burp_item("/users/99301", 403, "denied")]);
    let report = dir.join("report.md");

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");
    let out = Command::new(env!("CARGO_BIN_EXE_redactor"))
        .arg("compare")
        .args([&high, &low])
        .args(["-c", &format!("{root}/config.toml")])
        .args(["--labels", "owner,guest", "--report-out"])
        .arg(&report)
        .env("REDACTOR_CONFIG_DIR", "/nonexistent")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let report = std::fs::read_to_string(&report).unwrap();
    assert!(report.contains("owner vs guest"), "{report}");
    assert!(report.contains("GET /users/{id}"), "{report}");
    assert!(
        report.contains("replay these with guest's session"),
        "{report}"
    );
    for secret in [
        "globexbank",
        "88127",
        "99301",
        "maria.gomez",
        "a7f3e9c1d5b2084f",
    ] {
        assert!(!report.contains(secret), "{secret} leaked:\n{report}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn compare_rejects_files_that_are_not_burp_exports() {
    let dir = std::env::temp_dir().join(format!("redactor-cli-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.xml");
    std::fs::write(&bad, "<html></html>").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_redactor"))
        .arg("compare")
        .args([&bad, &bad])
        .env("REDACTOR_CONFIG_DIR", &dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a Burp site map export"), "{stderr}");
    std::fs::remove_dir_all(&dir).unwrap();
}
