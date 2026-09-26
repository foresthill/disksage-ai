//! BYOK API keys in the OS keychain (macOS Keychain / Windows Credential Manager
//! / Linux Secret Service), via the `keyring` crate. Keys are NEVER written to
//! the config file or any log — DiskSage's rule is "API keys in the keychain,
//! never plaintext on disk".
//!
//! Accounts are the env-var names the AI layer already understands, so a key set
//! here is a drop-in for `ANTHROPIC_API_KEY` / `OPENROUTER_API_KEY`.

use keyring::Entry;

const SERVICE: &str = "disksage";

/// The keychain accounts we manage (also the matching env-var names).
pub const ANTHROPIC: &str = "ANTHROPIC_API_KEY";
pub const OPENROUTER: &str = "OPENROUTER_API_KEY";

fn entry(account: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, account).map_err(|e| e.to_string())
}

/// Read a stored secret, or None if unset / unavailable.
pub fn get(account: &str) -> Option<String> {
    entry(account)
        .ok()?
        .get_password()
        .ok()
        .filter(|s| !s.is_empty())
}

/// True if a non-empty secret is stored for the account.
pub fn has(account: &str) -> bool {
    get(account).is_some()
}

/// Store (or replace) a secret in the OS keychain.
pub fn set(account: &str, secret: &str) -> Result<(), String> {
    entry(account)?
        .set_password(secret)
        .map_err(|e| e.to_string())
}

/// Remove a stored secret; treats "not there" as success (idempotent).
pub fn delete(account: &str) -> Result<(), String> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
