//! API-key storage backed by the operating system credential store.
//!
//! - Windows: Credential Manager
//! - macOS: Keychain
//! - Linux: Secret Service (GNOME Keyring / KWallet)
//!
//! Keys are never written to `settings.json` and never sent to the webview.
//! If the platform store cannot be opened (for example a headless Linux session
//! without a Secret Service provider), keys are held in memory for this process
//! only and [`CredentialStatus`] explains the degraded mode to the UI.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

/// Must stay stable: it is the service name used for every stored key.
pub const SERVICE: &str = "app.inkwell.viewer";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    /// True when the OS credential store is usable.
    pub available: bool,
    /// Human-readable backend name, e.g. "Windows Credential Manager".
    pub backend: String,
    /// Why the native store is unavailable, when it is.
    pub detail: Option<String>,
}

pub struct Credentials {
    memory: Mutex<HashMap<String, String>>,
    status: CredentialStatus,
}

impl Credentials {
    pub fn new() -> Self {
        // Initializes the platform store once and reports whether it works,
        // without creating any credential.
        let status = match keyring::Entry::store_status() {
            Ok(()) => CredentialStatus {
                available: true,
                backend: native_backend().to_string(),
                detail: None,
            },
            Err(err) => CredentialStatus {
                available: false,
                backend: "In-memory session store".to_string(),
                detail: Some(unavailable_detail(err)),
            },
        };
        if !status.available {
            if let Some(detail) = &status.detail {
                eprintln!("inkwell: credential store unavailable: {detail}");
            }
        }
        Self {
            memory: Mutex::new(HashMap::new()),
            status,
        }
    }

    pub fn status(&self) -> &CredentialStatus {
        &self.status
    }

    pub fn has(&self, provider: &str) -> bool {
        self.get(provider)
            .ok()
            .flatten()
            .is_some_and(|key| !key.trim().is_empty())
    }

    pub fn get(&self, provider: &str) -> Result<Option<String>, String> {
        if !self.status.available {
            return Ok(self.session_get(provider));
        }

        match keyring::Entry::new(SERVICE, provider) {
            Ok(entry) => match entry.get_password() {
                Ok(secret) => Ok(Some(secret)),
                Err(keyring::Error::NoEntry) => Ok(self.session_get(provider)),
                Err(err) => Err(describe("read", provider, &err)),
            },
            Err(err) => Err(describe("read", provider, &err)),
        }
    }

    pub fn set(&self, provider: &str, secret: &str) -> Result<(), String> {
        let secret = secret.trim();
        if secret.is_empty() {
            return self.delete(provider);
        }

        if !self.status.available {
            self.set_session_only(provider, secret);
            return Ok(());
        }

        match keyring::Entry::new(SERVICE, provider) {
            Ok(entry) => entry
                .set_password(secret)
                .map_err(|err| describe("save", provider, &err)),
            Err(err) => Err(describe("save", provider, &err)),
        }
    }

    pub fn delete(&self, provider: &str) -> Result<(), String> {
        if let Ok(mut memory) = self.memory.lock() {
            memory.remove(provider);
        }
        if !self.status.available {
            return Ok(());
        }

        match keyring::Entry::new(SERVICE, provider) {
            Ok(entry) => match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(describe("delete", provider, &err)),
            },
            Err(err) => Err(describe("delete", provider, &err)),
        }
    }

    /// Keeps a key for this process only. Used when migrating a legacy
    /// plaintext key and the OS store rejects the write, so the key is not
    /// lost for the current session and never written back to disk.
    pub fn set_session_only(&self, provider: &str, secret: &str) {
        if let Ok(mut memory) = self.memory.lock() {
            memory.insert(provider.to_string(), secret.to_string());
        }
    }

    fn session_get(&self, provider: &str) -> Option<String> {
        self.memory
            .lock()
            .ok()
            .and_then(|memory| memory.get(provider).cloned())
    }

    /// Credentials that never touch the OS store. Test-only.
    #[cfg(test)]
    pub fn memory_only() -> Self {
        Self {
            memory: Mutex::new(HashMap::new()),
            status: CredentialStatus {
                available: false,
                backend: "In-memory session store".to_string(),
                detail: Some("Memory-only credential store (tests).".to_string()),
            },
        }
    }
}

fn native_backend() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else {
        "Secret Service (GNOME Keyring / KWallet)"
    }
}

fn describe(action: &str, provider: &str, err: &keyring::Error) -> String {
    format!(
        "Could not {action} the API key for '{provider}' in {}: {err}",
        native_backend()
    )
}

fn unavailable_detail(err: &keyring::Error) -> String {
    let mut detail = match err {
        keyring::Error::NoStorageAccess(inner) => {
            format!("The system credential store is locked or inaccessible: {inner}")
        }
        keyring::Error::PlatformFailure(inner) => {
            format!("The system credential store is unavailable: {inner}")
        }
        keyring::Error::NoDefaultStore => {
            "No system credential store is available on this system.".to_string()
        }
        other => format!("The system credential store could not be initialised: {other}"),
    };
    if cfg!(target_os = "linux") {
        detail.push_str(" Install and unlock GNOME Keyring or KWallet, then restart Inkwell.");
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip() {
        let credentials = Credentials::memory_only();
        assert!(!credentials.has("openai"));
        credentials.set("openai", "sk-test").unwrap();
        assert!(credentials.has("openai"));
        assert_eq!(
            credentials.get("openai").unwrap().as_deref(),
            Some("sk-test")
        );
        credentials.delete("openai").unwrap();
        assert!(!credentials.has("openai"));
    }

    #[test]
    fn blank_keys_delete_instead_of_storing() {
        let credentials = Credentials::memory_only();
        credentials.set("openai", "sk-test").unwrap();
        credentials.set("openai", "   ").unwrap();
        assert!(!credentials.has("openai"));
    }

    #[test]
    fn keys_are_trimmed() {
        let credentials = Credentials::memory_only();
        credentials.set("openai", "  sk-test \n").unwrap();
        assert_eq!(
            credentials.get("openai").unwrap().as_deref(),
            Some("sk-test")
        );
    }
}

