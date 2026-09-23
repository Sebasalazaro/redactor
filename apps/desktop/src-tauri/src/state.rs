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

use crate::ai::Ai;
use crate::memory::{MemoryDto, Meter};
use crate::settings::AppSettings;
use crate::vault::{self, Vaults};

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

/// Why the last redaction is gone, so the overview can say so instead of
/// looking empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Forgotten {
    /// Older than `forget_last_after_minutes`.
    Expired,
    /// "Forget" in the overview.
    Cleared,
    /// "Free memory" in the overview.
    Freed,
    /// Keeping the last redaction was turned off.
    Disabled,
}

pub struct AppState {
    pub store: Store,
    pub ai: Ai,
    pub vaults: Vaults,
    settings: Mutex<AppSettings>,
    redactor: Mutex<Arc<Redactor>>,
    /// Redaction waiting in the review popup.
    pending: Mutex<Option<Redaction>>,
    last: Mutex<Option<LastRedaction>>,
    forgotten: Mutex<Option<(Forgotten, Instant)>>,
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
            ai: Ai::new(&store),
            vaults: Vaults::default(),
            store,
            settings: Mutex::new(settings),
            redactor: Mutex::new(Arc::new(Redactor::new(&Default::default()))),
            pending: Mutex::new(None),
            last: Mutex::new(None),
            forgotten: Mutex::new(None),
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
            self.clear_last(Forgotten::Disabled);
        }
        self.reload();
        Ok(())
    }

    /// Vault profile of the active engagement.
    pub fn profile(&self) -> String {
        vault::profile_of(self.settings().active_engagement.as_deref())
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
            *lock(&self.forgotten) = None;
        }
    }

    /// Turns the pending review into the last redaction, with the text the
    /// user confirmed. Returns false if there was nothing pending.
    pub fn complete_review(&self, text: &str) -> bool {
        let engagement = self.settings().active_engagement;
        let last = self.with_pending(|pending| {
            pending.map(|r| LastRedaction::new(text.to_string(), r, engagement))
        });
        self.set_pending(None);
        match last {
            Some(last) => {
                self.record(last);
                true
            }
            None => false,
        }
    }

    pub fn last(&self) -> Option<LastRedaction> {
        self.forget_expired();
        lock(&self.last).clone()
    }

    pub fn clear_last(&self, reason: Forgotten) {
        if lock(&self.last).take().is_some() {
            *lock(&self.forgotten) = Some((reason, Instant::now()));
        }
    }

    /// Why and how long ago the last redaction was dropped, if it was.
    pub fn forgotten(&self) -> Option<(Forgotten, Duration)> {
        lock(&self.forgotten).map(|(reason, at)| (reason, at.elapsed()))
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
            *lock(&self.forgotten) = Some((Forgotten::Expired, Instant::now()));
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
        lock(&self.meter).snapshot(self.ai.worker_pid())
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
        state.clear_last(Forgotten::Cleared);
        assert!(state.last().is_none());
        assert_eq!(state.forgotten().unwrap().0, Forgotten::Cleared);
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
        assert_eq!(state.forgotten().unwrap().0, Forgotten::Disabled);
        state.record(last(&state, "not kept"));
        assert!(state.last().is_none());
        std::fs::remove_dir_all(state.store.dir()).ok();
    }

    #[test]
    fn confirmed_review_becomes_the_last_redaction() {
        let state = state("confirm");
        assert!(!state.complete_review("nothing pending"));
        state.set_pending(Some(state.redactor().redact("GET /users/88127 HTTP/1.1")));
        assert!(state.complete_review("GET /users/8812* HTTP/1.1"));
        let last = state.last().unwrap();
        assert_eq!(last.output, "GET /users/8812* HTTP/1.1");
        assert_eq!(last.categories, [(redactor_core::Category::Id, 1)]);
        assert!(state.with_pending(|p| p.is_none()));
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
