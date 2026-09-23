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
