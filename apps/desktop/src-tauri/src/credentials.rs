#[cfg(test)]
use std::{collections::BTreeMap, sync::Mutex};

use thiserror::Error;

const SERVICE: &str = "com.deskaide.app";

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential storage is unavailable")]
    Unavailable,
    #[error("credential operation failed")]
    OperationFailed,
}

pub trait CredentialStore: Send + Sync {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), CredentialError>;
    fn get(&self, profile_id: &str) -> Result<Option<String>, CredentialError>;
    fn delete(&self, profile_id: &str) -> Result<(), CredentialError>;

    fn exists(&self, profile_id: &str) -> Result<bool, CredentialError> {
        self.get(profile_id).map(|value| value.is_some())
    }
}

#[cfg(windows)]
#[derive(Debug, Default)]
pub struct SystemCredentialStore;

#[cfg(windows)]
impl SystemCredentialStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(profile_id: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(SERVICE, &format!("model-profile:{profile_id}"))
            .map_err(|_| CredentialError::Unavailable)
    }
}

#[cfg(windows)]
impl CredentialStore for SystemCredentialStore {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), CredentialError> {
        Self::entry(profile_id)?
            .set_password(secret)
            .map_err(|_| CredentialError::OperationFailed)
    }

    fn get(&self, profile_id: &str) -> Result<Option<String>, CredentialError> {
        match Self::entry(profile_id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(CredentialError::OperationFailed),
        }
    }

    fn delete(&self, profile_id: &str) -> Result<(), CredentialError> {
        match Self::entry(profile_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError::OperationFailed),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
    values: Mutex<BTreeMap<String, String>>,
}

#[cfg(test)]
impl CredentialStore for MemoryCredentialStore {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), CredentialError> {
        self.values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(profile_id.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get(&self, profile_id: &str) -> Result<Option<String>, CredentialError> {
        Ok(self
            .values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(profile_id)
            .cloned())
    }

    fn delete(&self, profile_id: &str) -> Result<(), CredentialError> {
        self.values
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(profile_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_exposes_only_presence_without_calling_get() {
        let store = MemoryCredentialStore::default();
        assert!(!store.exists("one").unwrap());
        store.set("one", "secret").unwrap();
        assert!(store.exists("one").unwrap());
        store.delete("one").unwrap();
        assert!(!store.exists("one").unwrap());
    }
}
