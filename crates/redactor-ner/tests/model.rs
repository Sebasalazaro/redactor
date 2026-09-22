//! Runs the real model when it is installed (scripts/fetch-model.sh);
//! skipped otherwise, e.g. in CI. Point `REDACTOR_MODEL_DIR` elsewhere to
//! test another model.

use std::path::PathBuf;
use std::time::Instant;

use redactor_ner::{DEFAULT_LABELS, DEFAULT_THRESHOLD, Entity, Model};

fn model_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("REDACTOR_MODEL_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".config/redactor/models/gliner-pii-edge-v1.0"))
        })?;
    dir.join("model.onnx").exists().then_some(dir)
}

fn texts<'a>(text: &'a str, entities: &[Entity]) -> Vec<(&'a str, String)> {
    entities
        .iter()
        .map(|e| (&text[e.start..e.end], e.label.clone()))
        .collect()
}

#[test]
fn finds_people_and_organizations() {
    let Some(dir) = model_dir() else {
        eprintln!("model not installed, skipping");
        return;
    };
    let started = Instant::now();
    let mut model = Model::load(&dir).unwrap();
    let loaded = started.elapsed();

    let text = r#"{"note": "Maria Gonzalez from Initech Payments asked us to retest the refund flow. Call +1 555 0100 or ping jdoe_admin."}"#;
    let started = Instant::now();
    let found = model
        .detect(text, DEFAULT_LABELS, DEFAULT_THRESHOLD)
        .unwrap();
    println!(
        "load {loaded:?}, detect {:?}: {:?}",
        started.elapsed(),
        texts(text, &found)
    );

    let found = texts(text, &found);
    assert!(
        found.contains(&("Maria Gonzalez", "name".into())),
        "{found:?}"
    );
    assert!(
        found.contains(&("Initech Payments", "organization".into())),
        "{found:?}"
    );
    assert!(found.iter().any(|(t, _)| *t == "jdoe_admin"), "{found:?}");
}

#[test]
fn long_inputs_are_scanned_in_windows() {
    let Some(dir) = model_dir() else {
        return;
    };
    let mut model = Model::load(&dir).unwrap();
    // ~2 000 words: eight overlapping windows.
    let text = "Ticket assigned to Maria Gonzalez for review today. ".repeat(250);
    let started = Instant::now();
    let found = model.detect(&text, &["name"], DEFAULT_THRESHOLD).unwrap();
    println!(
        "{} words, {} names in {:?}",
        text.split_whitespace().count(),
        found.len(),
        started.elapsed()
    );
    // Windows overlap, but greedy decoding never reports the same words twice.
    assert!(found.windows(2).all(|w| w[0].end <= w[1].start));
    // Identical repeated sentences are an unnatural input; a few are missed.
    assert!(
        found.len() >= 240,
        "only {} of 250 names found",
        found.len()
    );
}
