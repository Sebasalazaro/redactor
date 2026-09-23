//! The app side of reversible mode: one encrypted vault per profile, loaded
//! on demand. The key is read from the OS keychain once and cached for the
//! session; Free memory drops both.

use std::sync::{Mutex, MutexGuard};

use redactor_core::store::GLOBAL_PROFILE;
use redactor_core::vault::VaultKey;
use redactor_core::{Restoration, Store, Vault};

/// A redacted value and the original it replaced.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Pair {
    pub replacement: String,
    pub original: String,
}

#[derive(Default)]
struct Loaded {
    key: Option<VaultKey>,
    /// Profile id and its decrypted vault.
    vault: Option<(String, Vault)>,
}

#[derive(Default)]
pub struct Vaults {
    loaded: Mutex<Loaded>,
}

pub fn profile_of(engagement: Option<&str>) -> String {
    engagement.unwrap_or(GLOBAL_PROFILE).to_string()
}

impl Vaults {
    /// Adds pairs to the profile's vault and saves it.
    pub fn remember(&self, store: &Store, profile: &str, pairs: &[Pair]) -> Result<(), String> {
        let mut loaded = self.lock();
        let key = Self::key(&mut loaded)?;
        let vault = Self::vault(&mut loaded, store, profile, &key)?;
        for pair in pairs {
            vault.record(&pair.replacement, &pair.original);
        }
        store
            .save_vault(profile, vault, &key)
            .map_err(|e| e.to_string())
    }

    pub fn restore(&self, store: &Store, profile: &str, text: &str) -> Result<Restoration, String> {
        let mut loaded = self.lock();
        let key = Self::key(&mut loaded)?;
        Ok(Self::vault(&mut loaded, store, profile, &key)?.restore(text))
    }

    /// Number of remembered values, if the vault can be read.
    pub fn len(&self, store: &Store, profile: &str) -> Result<usize, String> {
        let mut loaded = self.lock();
        let key = Self::key(&mut loaded)?;
        Ok(Self::vault(&mut loaded, store, profile, &key)?.len())
    }

    /// Deletes the profile's vault from disk and memory.
    pub fn forget(&self, store: &Store, profile: &str) -> Result<(), String> {
        let mut loaded = self.lock();
        if loaded.vault.as_ref().is_some_and(|(p, _)| p == profile) {
            loaded.vault = None;
        }
        store.delete_vault(profile).map_err(|e| e.to_string())
    }

    /// Drops the decrypted vault and the cached key.
    pub fn unload(&self) {
        *self.lock() = Loaded::default();
    }

    fn key(loaded: &mut Loaded) -> Result<VaultKey, String> {
        if loaded.key.is_none() {
            loaded.key = Some(redactor_core::keychain::vault_key().map_err(|e| e.to_string())?);
        }
        Ok(loaded.key.unwrap())
    }

    fn vault<'a>(
        loaded: &'a mut Loaded,
        store: &Store,
        profile: &str,
        key: &VaultKey,
    ) -> Result<&'a mut Vault, String> {
        if loaded.vault.as_ref().is_none_or(|(p, _)| p != profile) {
            let vault = store.load_vault(profile, key).map_err(|e| e.to_string())?;
            loaded.vault = Some((profile.to_string(), vault));
        }
        Ok(&mut loaded.vault.as_mut().unwrap().1)
    }

    fn lock(&self) -> MutexGuard<'_, Loaded> {
        self.loaded.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_restores_and_forgets_per_profile() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = std::env::temp_dir().join(format!("redactor-app-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(&dir);
        let vaults = Vaults::default();
        let pair = |r: &str, o: &str| Pair {
            replacement: r.into(),
            original: o.into(),
        };

        vaults
            .remember(
                &store,
                "globex-q3",
                &[pair("api.[CLIENT].example", "api.globexbank.example")],
            )
            .unwrap();
        let restored = vaults
            .restore(&store, "globex-q3", "GET https://api.[CLIENT].example/")
            .unwrap();
        assert_eq!(restored.text(), "GET https://api.globexbank.example/");
        assert_eq!(vaults.len(&store, GLOBAL_PROFILE).unwrap(), 0);

        vaults.forget(&store, "globex-q3").unwrap();
        assert_eq!(vaults.len(&store, "globex-q3").unwrap(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
