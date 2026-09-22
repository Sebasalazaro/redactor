//! Shared application state.

use std::sync::{Arc, Mutex, MutexGuard};

use redactor_core::{Redaction, Redactor, Store};

use crate::settings::AppSettings;

pub struct AppState {
    pub store: Store,
    settings: Mutex<AppSettings>,
    redactor: Mutex<Arc<Redactor>>,
    /// Redaction waiting in the review popup.
    pending: Mutex<Option<Redaction>>,
    /// Last configuration error, shown in the dashboard.
    error: Mutex<Option<String>>,
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
            error: Mutex::new(error),
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
        *lock(&self.settings) = settings;
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
}
