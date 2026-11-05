//! API keys of remote providers, kept in the OS keychain (Keychain on macOS, Credential Manager on
//! Windows, the Secret Service on Linux) under the provider's id, so they never touch the settings
//! file.

use anyhow::{Context, Result};

const SERVICE: &str = "dev.phucvinh57.makora";

pub trait SecretStore: Send + Sync {
    /// The key stored for provider `id`, `None` when there is none.
    fn get(&self, id: &str) -> Result<Option<String>>;
    fn set(&self, id: &str, key: &str) -> Result<()>;
    /// Forgets the key of provider `id`; nothing happens when there is none.
    fn delete(&self, id: &str) -> Result<()>;
}

pub struct KeyringSecrets;

fn entry(id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, id).context("the system keychain is not available")
}

impl SecretStore for KeyringSecrets {
    fn get(&self, id: &str) -> Result<Option<String>> {
        match entry(id)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err).context("reading the API key from the system keychain"),
        }
    }

    fn set(&self, id: &str, key: &str) -> Result<()> {
        entry(id)?
            .set_password(key)
            .context("saving the API key to the system keychain")
    }

    fn delete(&self, id: &str) -> Result<()> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err).context("removing the API key from the system keychain"),
        }
    }
}
