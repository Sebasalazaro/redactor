//! Shared application state.
//!
//! Sensitive text lives here for as short a time as possible: the pending
//! redaction only while the review popup is open, and the last redacted
//! output (already redacted, never the original) only if the user keeps it,
//! for a limited time.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use redactor_core::{Category, InputFormat, Redaction, Redactor, Store};

use crate::memory::{MemoryDto, Meter};
use crate::settings::AppSettings;

/// The last text written to the clipboard by the app.
#[derive(Debug, Clone)]
pub struct LastRedaction {
    pub output: String,
    pub format: InputFormat,
    pub categories: Vec<(Category, usize)>,
    pub engagement: Option<String>,
    pub at: Instant,
}

impl LastRedaction {
    pub fn new(output: String, redaction: &Redaction, engagement: Option<String>) -> Self {
        let mut categories: Vec<(Category, usize)> = Vec::new();
        for f in &redaction.findings {
            match categories.iter_mut().find(|(c, _)| *c == f.category) {
                Some((_, n)) => *n += 1,
                None => categories.push((f.category, 1)),
            }
        }
        categories.sort_by_key(|c| std::cmp::Reverse(c.1));
        Self {
            output,
            format: redaction.format,
            categories,
            engagement,
            at: Instant::now(),
        }
    }
}

pub struct AppState {
    pub store: Store,
    settings: Mutex<AppSettings>,
    redactor: Mutex<Arc<Redactor>>,
    /// Redaction waiting in the review popup.
    pending: Mutex<Option<Redaction>>,
    last: Mutex<Option<LastRedaction>>,
    meter: Mutex<Meter>,
    /// Last configuration error, shown in the dashboard.
    error: Mutex<Option<String>>,
    redactions: AtomicU64,
    started: Instant,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding a lock must not take the whole app down.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppState {
    pub fn new(store: Store) -> Self {
        let (settings, error) = match AppSettings::load(&store) {
            Ok(settings) => (settings, None),
            Err(e) => (AppSettings::default(), Some(e)),
        };
        let state = Self {
            store,
            settings: Mutex::new(settings),
            redactor: Mutex::new(Arc::new(Redactor::new(&Default::default()))),
            pending: Mutex::new(None),
            last: Mutex::new(None),
            meter: Mutex::new(Meter::new()),
            error: Mutex::new(error),
            redactions: AtomicU64::new(0),
            started: Instant::now(),
        };
        state.reload();
        state
    }

    /// Rebuilds the redactor from disk. On error the previous redactor stays
    /// active and the error is kept for the dashboard.
    pub fn reload(&self) {
        let engagement = self.settings().active_engagement;
        match self.store.resolve(engagement.as_deref()) {
            Ok(config) => {
                *lock(&self.redactor) = Arc::new(Redactor::new(&config));
                *lock(&self.error) = None;
            }
            Err(e) => *lock(&self.error) = Some(e.to_string()),
        }
    }

    pub fn redactor(&self) -> Arc<Redactor> {
        lock(&self.redactor).clone()
    }

    pub fn settings(&self) -> AppSettings {
        lock(&self.settings).clone()
    }

    pub fn set_settings(&self, settings: AppSettings) -> Result<(), String> {
        settings.save(&self.store)?;
        let keep_last = settings.keep_last;
        *lock(&self.settings) = settings;
        if !keep_last {
            self.clear_last();
        }
        self.reload();
        Ok(())
    }

    pub fn error(&self) -> Option<String> {
        lock(&self.error).clone()
    }

    pub fn set_pending(&self, redaction: Option<Redaction>) {
        *lock(&self.pending) = redaction;
    }

    pub fn with_pending<T>(&self, f: impl FnOnce(Option<&Redaction>) -> T) -> T {
        f(lock(&self.pending).as_ref())
    }

    /// Records text the app put on the clipboard.
    pub fn record(&self, last: LastRedaction) {
        self.redactions.fetch_add(1, Ordering::Relaxed);
        if self.settings().keep_last {
            *lock(&self.last) = Some(last);
        }
    }

    pub fn last(&self) -> Option<LastRedaction> {
        self.forget_expired();
        lock(&self.last).clone()
    }

    pub fn clear_last(&self) {
        *lock(&self.last) = None;
    }

    /// Drops the last redaction once it is older than the configured limit.
    pub fn forget_expired(&self) -> bool {
        let minutes = self.settings().forget_last_after_minutes;
        if minutes == 0 {
            return false;
        }
        let mut last = lock(&self.last);
        let expired = last
            .as_ref()
            .is_some_and(|l| l.at.elapsed() >= Duration::from_secs(u64::from(minutes) * 60));
        if expired {
            *last = None;
        }
        expired
    }

    pub fn redactions(&self) -> u64 {
        self.redactions.load(Ordering::Relaxed)
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn memory(&self) -> MemoryDto {
        lock(&self.meter).snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!("redactor-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        AppState::new(Store::new(dir))
    }

    fn last(state: &AppState, text: &str) -> LastRedaction {
        LastRedaction::new(text.into(), &state.redactor().redact(text), None)
    }

    #[test]
    fn keeps_only_the_latest_redaction() {
        let state = state("latest");
        state.record(last(&state, "first"));
        state.record(last(&state, "second"));
        assert_eq!(state.last().unwrap().output, "second");
        assert_eq!(state.redactions(), 2);
        state.clear_last();
        assert!(state.last().is_none());
        std::fs::remove_dir_all(state.store.dir()).ok();
    }

    #[test]
    fn keep_last_off_stores_nothing_and_clears() {
        let state = state("keep-off");
        state.record(last(&state, "kept"));
        let settings = AppSettings {
            keep_last: false,
            ..state.settings()
        };
        state.set_settings(settings).unwrap();
        assert!(state.last().is_none());
        state.record(last(&state, "not kept"));
        assert!(state.last().is_none());
        std::fs::remove_dir_all(state.store.dir()).ok();
    }

    #[test]
    fn pending_redaction_is_released() {
        let state = state("pending");
        state.set_pending(Some(state.redactor().redact("GET / HTTP/1.1")));
        assert!(state.with_pending(|p| p.is_some()));
        state.set_pending(None);
        assert!(state.with_pending(|p| p.is_none()));
        std::fs::remove_dir_all(state.store.dir()).ok();
    }
}
