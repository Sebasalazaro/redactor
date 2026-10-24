//! The app's binary doubles as the AI worker (`--ner-worker <model dir>`).
//! Needs the model (scripts/fetch-model.sh); skipped otherwise.

use std::path::PathBuf;
use std::process::Command;

use redactor_ner::worker::Client;
use redactor_ner::{DEFAULT_LABELS, DEFAULT_THRESHOLD};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

fn model_dir() -> Option<PathBuf> {
    let dir = redactor_core::store::default_dir()?.join("models/gliner-pii-edge-v1.0");
    dir.join("model.onnx").exists().then_some(dir)
}

/// Resident memory of a process, or `None` once it has exited.
fn rss_mb(pid: u32) -> Option<u64> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    system.process(pid).map(|p| p.memory() / (1024 * 1024))
}

#[test]
fn worker_detects_and_its_memory_goes_away_with_it() {
    let Some(dir) = model_dir() else {
        eprintln!("model not installed, skipping");
        return;
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_redactor-desktop"));
    command.arg("--ner-worker").arg(&dir);
    let mut client = Client::spawn(command).unwrap();
    let pid = client.pid();

    let text = "Maria Gonzalez from Initech Payments asked us to retest the refund flow.";
    let found = client
        .detect(text, DEFAULT_LABELS, DEFAULT_THRESHOLD)
        .unwrap();
    let names: Vec<&str> = found.iter().map(|e| &text[e.start..e.end]).collect();
    assert!(names.contains(&"Maria Gonzalez"), "{names:?}");
    let while_running = rss_mb(pid);

    drop(client);
    println!(
        "worker {pid}: {while_running:?} MB while running, gone after drop: {}",
        rss_mb(pid).is_none()
    );
    assert!(rss_mb(pid).is_none(), "the worker is still alive");
}

#[test]
fn worker_reports_a_missing_model() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_redactor-desktop"));
    command.arg("--ner-worker").arg("/nonexistent");
    let mut client = Client::spawn(command).unwrap();
    let error = client
        .detect("text", &["name"], 0.3)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not found"), "{error}");
}
