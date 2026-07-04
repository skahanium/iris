use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

use keyring::Entry;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const KEYRING_ACCOUNT: &str = "api_key";
#[cfg(debug_assertions)]
const API_KEY_BUNDLE_SERVICE: &str = "iris.dev.api_keys";
#[cfg(not(debug_assertions))]
const API_KEY_BUNDLE_SERVICE: &str = "iris.api_keys";
#[cfg(debug_assertions)]
const API_KEY_MARKER_PREFIX: &str = "credential_configured.dev.";
#[cfg(not(debug_assertions))]
const API_KEY_MARKER_PREFIX: &str = "credential_configured.";

static API_KEY_BUNDLE_CACHE: LazyLock<Mutex<Option<ApiKeyBundle>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
struct ApiKeyBundle {
    keys: BTreeMap<String, String>,
}

impl ApiKeyBundle {
    fn from_json(json: &str) -> AppResult<Self> {
        serde_json::from_str(json)
            .map_err(|e| AppError::msg(format!("API Key bundle corrupted: {e}")))
    }

    fn to_json(&self) -> AppResult<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    fn get(&self, service: &str) -> Option<&str> {
        self.keys
            .get(&canonical_service_id(service))
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
    }

    fn upsert(&mut self, service: &str, value: &str) {
        if let Some(mut old) = self
            .keys
            .insert(canonical_service_id(service), value.to_string())
        {
            old.zeroize();
        }
    }

    fn remove(&mut self, service: &str) {
        if let Some(mut value) = self.keys.remove(&canonical_service_id(service)) {
            value.zeroize();
        }
    }

    fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

impl Drop for ApiKeyBundle {
    fn drop(&mut self) {
        for value in self.keys.values_mut() {
            value.zeroize();
        }
    }
}

/// LLM 厂商凭据 ID（与前端 `llmCredentialService` 一致）。
pub fn llm_credential_service(provider: &str) -> String {
    format!("iris.llm.{provider}")
}

/// 将旧版 `iris/llm/openai` 等形式规范为 `iris.llm.openai`（Windows 凭据目标名不宜含 `/`）。
fn canonical_service_id(service: &str) -> String {
    if service.contains('/') {
        service.replace('/', ".")
    } else {
        service.to_string()
    }
}

fn entry_canonical(canonical: &str) -> AppResult<Entry> {
    Entry::new(canonical, KEYRING_ACCOUNT).map_err(Into::into)
}

fn entry_legacy(service: &str) -> AppResult<Entry> {
    Entry::new("iris", service).map_err(Into::into)
}

fn entry_error_to_keyring(error: AppError) -> keyring::Error {
    match error {
        AppError::Keyring(err) => err,
        other => {
            keyring::Error::PlatformFailure(Box::new(std::io::Error::other(other.to_string())))
        }
    }
}

#[cfg(target_os = "macos")]
fn set_secret_material(canonical: &str, value: &str) -> Result<(), keyring::Error> {
    match macos_protected_keychain::set_password(canonical, KEYRING_ACCOUNT, value) {
        Ok(()) => Ok(()),
        Err(_) => entry_canonical(canonical)
            .map_err(entry_error_to_keyring)?
            .set_password(value),
    }
}

#[cfg(not(target_os = "macos"))]
fn set_secret_material(canonical: &str, value: &str) -> Result<(), keyring::Error> {
    entry_canonical(canonical)
        .map_err(entry_error_to_keyring)?
        .set_password(value)
}

#[cfg(target_os = "macos")]
fn get_secret_material(canonical: &str) -> Result<String, keyring::Error> {
    match macos_protected_keychain::get_password(canonical, KEYRING_ACCOUNT) {
        Ok(value) => Ok(value),
        Err(keyring::Error::NoEntry) => entry_canonical(canonical)
            .map_err(entry_error_to_keyring)?
            .get_password(),
        Err(err) => Err(err),
    }
}

#[cfg(not(target_os = "macos"))]
fn get_secret_material(canonical: &str) -> Result<String, keyring::Error> {
    entry_canonical(canonical)
        .map_err(entry_error_to_keyring)?
        .get_password()
}

#[cfg(target_os = "macos")]
fn delete_secret_material(canonical: &str) -> Result<(), keyring::Error> {
    let protected = macos_protected_keychain::delete_password(canonical, KEYRING_ACCOUNT);
    let legacy = entry_canonical(canonical)
        .map_err(entry_error_to_keyring)?
        .delete_credential();
    protected.or(legacy).or_else(|err| {
        if matches!(err, keyring::Error::NoEntry) {
            Ok(())
        } else {
            Err(err)
        }
    })
}

#[cfg(not(target_os = "macos"))]
fn delete_secret_material(canonical: &str) -> Result<(), keyring::Error> {
    entry_canonical(canonical)
        .map_err(entry_error_to_keyring)?
        .delete_credential()
}

fn get_canonical_password_optional(canonical: &str) -> AppResult<Option<String>> {
    match get_secret_material(canonical) {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(target_os = "macos")]
mod macos_protected_keychain {
    use security_framework::base::Error as SecError;
    use security_framework::passwords::{
        delete_generic_password_options, generic_password, set_generic_password_options,
        AccessControlOptions, PasswordOptions,
    };
    use zeroize::Zeroizing;

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    fn options(service: &str, account: &str) -> PasswordOptions {
        PasswordOptions::new_generic_password(service, account)
    }

    fn protected_options(service: &str, account: &str) -> PasswordOptions {
        let mut options = options(service, account);
        options.set_access_control_options(AccessControlOptions::USER_PRESENCE);
        options
    }

    fn map_error(err: SecError) -> keyring::Error {
        if err.code() == ERR_SEC_ITEM_NOT_FOUND {
            keyring::Error::NoEntry
        } else {
            keyring::Error::PlatformFailure(Box::new(err))
        }
    }

    pub(super) fn set_password(
        service: &str,
        account: &str,
        value: &str,
    ) -> Result<(), keyring::Error> {
        set_generic_password_options(value.as_bytes(), protected_options(service, account))
            .map_err(map_error)
    }

    pub(super) fn get_password(service: &str, account: &str) -> Result<String, keyring::Error> {
        let bytes = Zeroizing::new(generic_password(options(service, account)).map_err(map_error)?);
        String::from_utf8(bytes.to_vec())
            .map_err(|err| keyring::Error::BadEncoding(err.into_bytes()))
    }

    pub(super) fn delete_password(service: &str, account: &str) -> Result<(), keyring::Error> {
        delete_generic_password_options(options(service, account)).map_err(map_error)
    }
}

/// Store a secret in the OS credential manager.
pub fn set_secret(service: &str, value: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    set_secret_material(&canonical, value)?;
    tracing::debug!("credential stored for {canonical}");
    Ok(())
}

/// Read a secret from the OS credential manager.
pub fn get_secret(service: &str) -> AppResult<String> {
    let canonical = canonical_service_id(service);
    match get_secret_material(&canonical) {
        Ok(password) => return Ok(password),
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(e.into()),
    }
    if service.contains('/') {
        return entry_legacy(service)?.get_password().map_err(Into::into);
    }
    Err(AppError::msg(format!("凭据不存在: {canonical}")))
}

/// Delete a stored secret.
pub fn delete_secret(service: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    let _ = delete_secret_material(&canonical);
    if service.contains('/') {
        if let Ok(entry) = entry_legacy(service) {
            let _ = entry.delete_credential();
        }
    }
    Ok(())
}

/// Check if a secret exists without logging its value.
pub fn has_secret(service: &str) -> bool {
    get_secret(service).is_ok()
}

fn cache_lock() -> AppResult<std::sync::MutexGuard<'static, Option<ApiKeyBundle>>> {
    API_KEY_BUNDLE_CACHE
        .lock()
        .map_err(|_| AppError::msg("Credential cache lock error"))
}

fn read_api_key_bundle_uncached() -> AppResult<ApiKeyBundle> {
    match get_canonical_password_optional(API_KEY_BUNDLE_SERVICE)? {
        Some(json) => {
            let json = Zeroizing::new(json);
            let bundle = ApiKeyBundle::from_json(&json)?;
            migrate_api_key_bundle_storage_policy(&json);
            Ok(bundle)
        }
        None => Ok(ApiKeyBundle::default()),
    }
}

#[cfg(target_os = "macos")]
fn migrate_api_key_bundle_storage_policy(json: &str) {
    let _ = macos_protected_keychain::set_password(API_KEY_BUNDLE_SERVICE, KEYRING_ACCOUNT, json);
}

#[cfg(not(target_os = "macos"))]
fn migrate_api_key_bundle_storage_policy(_json: &str) {}

fn read_api_key_bundle_cached() -> AppResult<ApiKeyBundle> {
    let mut guard = cache_lock()?;
    if let Some(bundle) = guard.as_ref() {
        return Ok(bundle.clone());
    }
    let bundle = read_api_key_bundle_uncached()?;
    *guard = Some(bundle.clone());
    Ok(bundle)
}

fn store_api_key_bundle(bundle: &ApiKeyBundle) -> AppResult<()> {
    if bundle.is_empty() {
        delete_secret(API_KEY_BUNDLE_SERVICE)?;
    } else {
        let json = Zeroizing::new(bundle.to_json()?);
        set_secret(API_KEY_BUNDLE_SERVICE, &json)?;
    }
    *cache_lock()? = Some(bundle.clone());
    Ok(())
}

pub fn credential_unlock_session() -> AppResult<()> {
    let bundle = read_api_key_bundle_uncached()?;
    *cache_lock()? = Some(bundle);
    Ok(())
}

pub fn credential_lock_session() -> AppResult<()> {
    *cache_lock()? = None;
    crate::cas::encryption::clear_cas_key_cache()?;
    Ok(())
}

fn api_key_marker_key(service: &str) -> String {
    format!("{API_KEY_MARKER_PREFIX}{}", canonical_service_id(service))
}

#[cfg(debug_assertions)]
fn get_legacy_api_key_secret(_canonical: &str) -> AppResult<String> {
    Err(AppError::msg("凭据不存在"))
}

#[cfg(not(debug_assertions))]
fn get_legacy_api_key_secret(canonical: &str) -> AppResult<String> {
    get_secret(canonical)
}

pub fn mark_api_key_configured(db: &Database, service: &str) -> AppResult<()> {
    let key = api_key_marker_key(service);
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, "true"],
        )?;
        Ok(())
    })
}

pub fn clear_api_key_configured(db: &Database, service: &str) -> AppResult<()> {
    let key = api_key_marker_key(service);
    db.with_conn(|conn| {
        conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    })
}

pub fn api_key_configured(db: &Database, service: &str) -> AppResult<bool> {
    let key = api_key_marker_key(service);
    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    })
}

pub fn set_api_key(db: &Database, service: &str, value: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    let mut bundle = read_api_key_bundle_cached()?;
    bundle.upsert(&canonical, value);
    store_api_key_bundle(&bundle)?;
    let stored = read_api_key_bundle_cached()?;
    if stored.get(&canonical).is_none() {
        return Err(AppError::msg(format!(
            "API Key 写入后校验失败: {canonical}"
        )));
    }
    mark_api_key_configured(db, &canonical)
}

pub fn get_api_key(db: &Database, service: &str) -> AppResult<String> {
    let canonical = canonical_service_id(service);
    let mut bundle = match read_api_key_bundle_cached() {
        Ok(bundle) => bundle,
        Err(bundle_err) => {
            return match get_legacy_api_key_secret(&canonical) {
                Ok(value) => {
                    mark_api_key_configured(db, &canonical)?;
                    Ok(value)
                }
                Err(_) => Err(bundle_err),
            };
        }
    };
    if let Some(value) = bundle.get(&canonical) {
        mark_api_key_configured(db, &canonical)?;
        return Ok(value.to_string());
    }

    let legacy = match get_legacy_api_key_secret(&canonical) {
        Ok(value) => value,
        Err(e) => {
            clear_api_key_configured(db, &canonical)?;
            return Err(e);
        }
    };
    bundle.upsert(&canonical, &legacy);
    store_api_key_bundle(&bundle)?;
    mark_api_key_configured(db, &canonical)?;
    Ok(legacy)
}

pub fn delete_api_key(db: &Database, service: &str) -> AppResult<()> {
    let canonical = canonical_service_id(service);
    let mut bundle = read_api_key_bundle_cached()?;
    bundle.remove(&canonical);
    store_api_key_bundle(&bundle)?;
    let _ = delete_secret(&canonical);
    clear_api_key_configured(db, &canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn canonical_id_replaces_slashes() {
        assert_eq!(
            canonical_service_id("iris/llm/deepseek"),
            "iris.llm.deepseek"
        );
    }

    #[test]
    fn api_key_bundle_serializes_multiple_services() {
        let mut bundle = ApiKeyBundle::default();
        bundle.upsert("iris.llm.deepseek", "deepseek-key");
        bundle.upsert("iris.llm.minimax", "minimax-key");

        let json = bundle.to_json().expect("serialize");
        let decoded = ApiKeyBundle::from_json(&json).expect("deserialize");

        assert_eq!(decoded.get("iris.llm.deepseek"), Some("deepseek-key"));
        assert_eq!(decoded.get("iris.llm.minimax"), Some("minimax-key"));
        assert_eq!(decoded.get("iris.llm.mimo"), None);
    }

    #[test]
    fn api_key_configured_marker_roundtrips_without_secret_value() {
        let db = Database::open_in_memory().expect("mem db");

        mark_api_key_configured(&db, "iris.llm.deepseek").expect("mark");
        assert!(api_key_configured(&db, "iris.llm.deepseek").expect("configured"));

        let stored = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    [api_key_marker_key("iris.llm.deepseek")],
                    |row| row.get::<_, String>(0),
                )
                .map_err(Into::into)
            })
            .expect("stored marker");
        assert!(!stored.contains("deepseek-key"));

        clear_api_key_configured(&db, "iris.llm.deepseek").expect("clear");
        assert!(!api_key_configured(&db, "iris.llm.deepseek").expect("configured"));
    }

    #[test]
    fn get_api_key_clears_stale_marker_when_bundle_and_legacy_secret_are_missing() {
        let db = Database::open_in_memory().expect("mem db");
        let service = format!("iris.llm.missing_{}", uuid::Uuid::new_v4());
        *cache_lock().expect("cache") = Some(ApiKeyBundle::default());

        mark_api_key_configured(&db, &service).expect("mark");
        let err = get_api_key(&db, &service).expect_err("missing secret");

        assert!(err.to_string().contains("凭据不存在"));
        assert!(!api_key_configured(&db, &service).expect("marker cleared"));
    }

    #[test]
    fn credential_lock_session_clears_api_key_bundle_cache() {
        let db = Database::open_in_memory().expect("mem db");
        let service = format!("iris.llm.locked_{}", uuid::Uuid::new_v4());
        let mut bundle = ApiKeyBundle::default();
        bundle.upsert(&service, "cached-key");
        *cache_lock().expect("cache") = Some(bundle);

        assert_eq!(
            get_api_key(&db, &service).expect("cached key"),
            "cached-key"
        );

        credential_lock_session().expect("lock session");
        let err = get_api_key(&db, &service).expect_err("cache cleared");

        assert!(err.to_string().contains("凭据不存在"));
    }

    #[test]
    fn credential_unlock_session_warms_empty_api_key_bundle_cache() {
        *cache_lock().expect("cache") = None;

        credential_unlock_session().expect("unlock session");

        assert!(cache_lock().expect("cache").is_some());
    }

    #[test]
    #[ignore = "requires OS credential store; run locally: cargo test credential_roundtrip -- --ignored"]
    fn credential_roundtrip() {
        let id = format!("iris.test.{}", uuid::Uuid::new_v4());
        set_secret(&id, "test-secret-value").expect("set");
        assert!(has_secret(&id));
        assert_eq!(get_secret(&id).expect("get"), "test-secret-value");
        delete_secret(&id).expect("delete");
        assert!(!has_secret(&id));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS Keychain user-presence prompt; run locally with --ignored"]
    fn macos_user_presence_credential_roundtrip() {
        let id = format!("iris.test.touchid.{}", uuid::Uuid::new_v4());
        set_secret(&id, "test-secret-value").expect("set protected");
        assert_eq!(get_secret(&id).expect("get protected"), "test-secret-value");
        delete_secret(&id).expect("delete protected");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS Keychain access; run locally with --ignored"]
    fn macos_legacy_keyring_secret_remains_readable() {
        let id = format!("iris.test.legacy.{}", uuid::Uuid::new_v4());
        entry_canonical(&id)
            .expect("legacy entry")
            .set_password("legacy-secret-value")
            .expect("set legacy");

        assert_eq!(get_secret(&id).expect("get legacy"), "legacy-secret-value");

        delete_secret(&id).expect("delete");
    }
}
