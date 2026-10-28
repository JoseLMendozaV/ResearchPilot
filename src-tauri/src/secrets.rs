//! API keys live in the operating system's secure store (macOS Keychain),
//! never in SQLite, settings JSON or logs. Only presence is ever reported.

use crate::error::{AppError, AppResult};

const SERVICE: &str = "com.researchpilot.desktop";

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> AppResult<Option<String>>;
    fn set(&self, name: &str, value: &str) -> AppResult<()>;
    fn delete(&self, name: &str) -> AppResult<()>;
}

pub fn api_key_name(provider: &str) -> AppResult<&'static str> {
    match provider {
        "openai" => Ok("openai-api-key"),
        "anthropic" => Ok("anthropic-api-key"),
        _ => Err(AppError::Validation(
            "Proveedor externo no reconocido".into(),
        )),
    }
}

pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn get(&self, name: &str) -> AppResult<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, name).map_err(keychain_error)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keychain_error(error)),
        }
    }

    fn set(&self, name: &str, value: &str) -> AppResult<()> {
        keyring::Entry::new(SERVICE, name)
            .and_then(|entry| entry.set_password(value))
            .map_err(keychain_error)
    }

    fn delete(&self, name: &str) -> AppResult<()> {
        match keyring::Entry::new(SERVICE, name).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keychain_error(error)),
        }
    }
}

fn keychain_error(error: keyring::Error) -> AppError {
    AppError::Ai(format!(
        "No se pudo acceder al Llavero del sistema: {error}"
    ))
}

/// In-memory store for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<std::collections::HashMap<String, String>>);

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().expect("secrets lock").get(name).cloned())
    }

    fn set(&self, name: &str, value: &str) -> AppResult<()> {
        self.0
            .lock()
            .expect("secrets lock")
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> AppResult<()> {
        self.0.lock().expect("secrets lock").remove(name);
        Ok(())
    }
}
