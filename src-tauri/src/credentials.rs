use keyring::Entry;

use crate::error::AppResult;

/// Store a secret in the OS credential manager.
pub fn set_secret(service: &str, value: &str) -> AppResult<()> {
    let entry = Entry::new("iris", service)?;
    entry.set_password(value)?;
    Ok(())
}

/// Read a secret from the OS credential manager.
pub fn get_secret(service: &str) -> AppResult<String> {
    let entry = Entry::new("iris", service)?;
    entry.get_password().map_err(Into::into)
}

/// Delete a stored secret.
pub fn delete_secret(service: &str) -> AppResult<()> {
    let entry = Entry::new("iris", service)?;
    entry.delete_credential()?;
    Ok(())
}

/// Check if a secret exists without logging its value.
pub fn has_secret(service: &str) -> bool {
    get_secret(service).is_ok()
}
