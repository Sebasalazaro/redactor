//! On-demand AI detection ("Deep scan") with the local GLiNER model.
//!
//! The model runs in a worker process (this same executable, started with
//! `--ner-worker`) created on first use and ended after two idle minutes or
//! by Free memory. Ending the process is the only way to get all of ONNX
//! Runtime's memory back, so the app costs nothing extra while unused.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use redactor_core::{Category, Redactor, Store};
use redactor_ner::worker::Client;
use redactor_ner::{DEFAULT_LABELS, DEFAULT_THRESHOLD};
use serde::Serialize;

/// Folder, under the config dir, that `scripts/fetch-model.sh` fills.
const MODEL: &str = "models/gliner-pii-edge-v1.0";
const IDLE: Duration = Duration::from_secs(120);

/// An entity found by the model, positioned in UTF-16 code units so the
/// review page (JavaScript strings) can use the offsets directly.
#[derive(Debug, Serialize)]
pub struct AiEntity {
    pub start: usize,
    pub end: usize,
    pub label: String,
    pub score: f32,
    pub category: Category,
    pub replacement: String,
}

pub struct Ai {
    dir: PathBuf,
    worker: Mutex<Option<(Client, Instant)>>,
}

impl Ai {
    pub fn new(store: &Store) -> Self {
        Self {
            dir: store.dir().join(MODEL),
            worker: Mutex::new(None),
        }
    }

    pub fn installed(&self) -> bool {
        self.dir.join("model.onnx").exists()
    }

    pub fn loaded(&self) -> bool {
        self.lock().is_some()
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Process id of the running worker, for memory accounting.
    pub fn worker_pid(&self) -> Option<u32> {
        self.lock().as_ref().map(|(client, _)| client.pid())
    }

    /// Finds names, organizations, usernames... that patterns missed.
    pub fn scan(&self, text: &str, redactor: &Redactor) -> Result<Vec<AiEntity>, String> {
        if !self.installed() {
            return Err("the AI model is not installed (run scripts/fetch-model.sh)".into());
        }
        let mut slot = self.lock();
        if slot.is_none() {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let mut command = std::process::Command::new(exe);
            command.arg(crate::WORKER_FLAG).arg(&self.dir);
            let client = Client::spawn(command).map_err(|e| e.to_string())?;
            *slot = Some((client, Instant::now()));
        }
        let (client, used) = slot.as_mut().unwrap();
        *used = Instant::now();
        let entities = match client.detect(text, DEFAULT_LABELS, DEFAULT_THRESHOLD) {
            Ok(entities) => entities,
            Err(e) => {
                *slot = None; // a broken worker is restarted on the next scan
                return Err(e.to_string());
            }
        };

        let utf16 = Utf16Offsets::new(text);
        Ok(entities
            .into_iter()
            .map(|e| {
                let value = &text[e.start..e.end];
                let (category, replacement) = match e.label.as_str() {
                    "password" => (Category::Secret, redactor.mask_secret(value)),
                    _ => (Category::Pii, redactor.mask_pii(value)),
                };
                AiEntity {
                    start: utf16.of(e.start),
                    end: utf16.of(e.end),
                    label: e.label,
                    score: e.score,
                    category,
                    replacement,
                }
            })
            .collect())
    }

    /// Drops the model if unused for a while. Returns true if it was dropped.
    pub fn unload_if_idle(&self) -> bool {
        let mut slot = self.lock();
        let idle = slot
            .as_ref()
            .is_some_and(|(_, used)| used.elapsed() >= IDLE);
        if idle {
            *slot = None;
        }
        idle
    }

    pub fn unload(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(Client, Instant)>> {
        self.worker.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Byte offset -> UTF-16 offset, for character boundaries of one text.
struct Utf16Offsets {
    /// (byte offset, utf-16 offset) at every character boundary.
    table: Vec<(usize, usize)>,
}

impl Utf16Offsets {
    fn new(text: &str) -> Self {
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut units = 0;
        for (i, c) in text.char_indices() {
            table.push((i, units));
            units += c.len_utf16();
        }
        table.push((text.len(), units));
        Self { table }
    }

    fn of(&self, byte: usize) -> usize {
        let i = self.table.partition_point(|&(b, _)| b < byte);
        self.table[i].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_byte_offsets_to_utf16() {
        let text = "Añil 😀 ok";
        let map = Utf16Offsets::new(text);
        assert_eq!(map.of(0), 0);
        assert_eq!(map.of(text.find('i').unwrap()), 2); // 'ñ' is 2 bytes, 1 unit
        assert_eq!(map.of(text.find("ok").unwrap()), 8); // emoji is 4 bytes, 2 units
        assert_eq!(map.of(text.len()), text.encode_utf16().count());
    }
}
