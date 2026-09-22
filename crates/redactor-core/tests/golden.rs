//! Golden tests: every file in `tests/fixtures` is redacted with the example
//! configs and compared against `tests/expected/<name>`.
//!
//! Regenerate the expected files after an intended change with:
//!
//! ```sh
//! REDACTOR_BLESS=1 cargo test -p redactor-core --test golden
//! ```
//!
//! All fixtures are fictional (Globex Bank, `.example` domains, RFC 5737
//! addresses, AWS documentation keys).

use std::fs;
use std::path::Path;

use redactor_core::{Config, Redactor};

fn redactor() -> Redactor {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let global = Config::load(root.join("config.toml")).unwrap();
    let engagement = Config::load(root.join("engagement.toml")).unwrap();
    Redactor::new(&global.with_engagement(&engagement))
}

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&p).unwrap())
        })
        .collect();
    files.sort();
    files
}

#[test]
fn outputs_match_expected() {
    let redactor = redactor();
    let expected_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected");
    let bless = std::env::var_os("REDACTOR_BLESS").is_some();
    let mut failures = Vec::new();

    for (name, input) in fixtures() {
        let output = redactor.redact(&input).output;
        let path = expected_dir.join(&name);
        if bless {
            fs::write(&path, &output).unwrap();
            continue;
        }
        let expected = fs::read_to_string(&path).unwrap_or_default();
        if output != expected {
            failures.push(format!("--- {name}\n{output}"));
        }
    }
    assert!(
        failures.is_empty(),
        "outputs changed:\n{}",
        failures.join("\n")
    );
}

/// Values planted in the fixtures that must never survive redaction.
const PLANTED: &[&str] = &[
    "globex",
    "a7f3e9c1d5b2084f6e1a9c3d7b5f2e80",
    "0d9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a",
    "Qm9vdHN0cmFwVG9rZW4xMjM0NTY3OA",
    "0123456789abcdefABCDEF",
    "4c9e1f0a8b7d6e5f",
    "R3s3tT0k3n-9f8e7d6c5b4a",
    "cs_9a8b7c6d5e4f3a2b1c0d",
    "gbx_pk_1a2b3c4d5e6f7g8h",
    "opaque-7d1f2c9b8a3e4f5d6c7b8a9e0f1d2c3b",
    "S3cr3t-Pa55w0rd!",
    "Winter2026!",
    "Tr0ub4dor&3",
    "482913",
    "4111111111111111",
    "AKIAIOSFODNN7EXAMPLE",
    "MIIEowIBAAKCAQEA",
    "Zm9yLXRlc3RzLW9ubHktbm90LWEtcmVhbC1zaWduYXR1cmU",
    "f3b1c2d4-9a7e-4b21-8c3d-5e6f7a8b9c0d",
    "5b2c9e7a-1f3d-4c8b-9a6e-2d7f0b1c3e4a",
    "88127",
    "55012",
    "77120",
    "usr_8f7a6b5c",
    "juan.perez",
    "Juan Pérez",
    "María Gómez",
    "Carlos Ruiz",
    "Ana Torres",
    "qa.tester01",
    "qa.tester02",
    "svc_integration",
    "srv-db01",
    "MacBook-de-Pentester",
    "203.0.113.45",
    "198.51.100.23",
    "10.40.2.15",
    "10.40.7.3",
    "10.12.4.21",
    "+57 300 555 0199",
];

#[test]
fn planted_values_never_leak() {
    let redactor = redactor();
    for (name, input) in fixtures() {
        let output = redactor.redact(&input).output.to_lowercase();
        for value in PLANTED {
            assert!(
                !output.contains(&value.to_lowercase()),
                "{name} leaks {value:?}"
            );
        }
    }
}

#[test]
fn redaction_is_deterministic() {
    let redactor = redactor();
    for (_, input) in fixtures() {
        assert_eq!(
            redactor.redact(&input).output,
            redactor.redact(&input).output
        );
    }
}
