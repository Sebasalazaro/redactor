//! The vault key, kept in the OS keychain (macOS Keychain, Windows
//! Credential Manager, Linux kernel keyring). Created on first use; never
//! written to disk by redactor.

use crate::vault::{VaultError, VaultKey, generate_key};

const SERVICE: &str = "dev.redactor";
const ACCOUNT: &str = "vault-key";

/// The vault key, generated and stored on first use.
pub fn vault_key() -> Result<VaultKey, VaultError> {
    let keychain = |e: keyring::Error| VaultError::Keychain(e.to_string());
    let entry = keyring::Entry::new(SERVICE, ACCOUNT).map_err(keychain)?;
    match entry.get_secret() {
        Ok(bytes) => bytes
            .try_into()
            .map_err(|_| VaultError::Keychain("the stored key has the wrong size".into())),
        Err(keyring::Error::NoEntry) => {
            let key = generate_key();
            entry.set_secret(&key).map_err(keychain)?;
            Ok(key)
        }
        Err(e) => Err(keychain(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_key_on_first_use() {
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let key = vault_key().unwrap();
        assert_ne!(key, [0; 32]);
    }
}
