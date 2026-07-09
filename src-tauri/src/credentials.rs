use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::security::ipc_policy::validate_credential_service;

const SECRET_ACCOUNT: &str = "api_key";
const LOCAL_CREDENTIAL_DIR: &str = "credentials";
const LOCAL_MASTER_KEY_FILE: &str = "master.key";
const LOCAL_KEY_LEN: usize = 32;
const LOCAL_NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Available,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub service: String,
    pub state: CredentialState,
    pub configured: bool,
    pub checked_at: String,
}

trait CredentialBackend {
    fn set_password(&self, service: &str, account: &str, value: &str) -> AppResult<()>;
    fn get_password(&self, service: &str, account: &str) -> AppResult<Zeroizing<String>>;
    fn delete_password(&self, service: &str, account: &str) -> AppResult<()>;
}

struct LocalEncryptedCredentialBackend {
    root: PathBuf,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalCredentialRecord {
    version: u8,
    service_hash: String,
    ciphertext: String,
}

impl LocalEncryptedCredentialBackend {
    fn default() -> AppResult<Self> {
        let data_dir = std::env::var_os("IRIS_DATA_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| AppError::msg("IRIS_DATA_DIR is not configured"))?;
        Ok(Self {
            root: data_dir.join(LOCAL_CREDENTIAL_DIR),
        })
    }

    #[cfg(test)]
    fn new_for_test(root: PathBuf) -> Self {
        Self { root }
    }

    fn ensure_root(&self) -> AppResult<()> {
        fs::create_dir_all(&self.root)?;
        set_private_dir_permissions(&self.root)?;
        Ok(())
    }

    fn master_key(&self) -> AppResult<[u8; LOCAL_KEY_LEN]> {
        self.ensure_root()?;
        let path = self.root.join(LOCAL_MASTER_KEY_FILE);
        if path.is_file() {
            let encoded = fs::read_to_string(&path)?;
            let decoded = B64
                .decode(encoded.trim())
                .map_err(|_| AppError::Credential("local master key is corrupt".into()))?;
            if decoded.len() != LOCAL_KEY_LEN {
                return Err(AppError::Credential(
                    "local master key has invalid length".into(),
                ));
            }
            let mut key = [0u8; LOCAL_KEY_LEN];
            key.copy_from_slice(&decoded);
            return Ok(key);
        }

        let mut key = [0u8; LOCAL_KEY_LEN];
        OsRng.fill_bytes(&mut key);
        write_private_file(&path, B64.encode(key).as_bytes())?;
        Ok(key)
    }

    fn credential_path(&self, service: &str, account: &str) -> AppResult<PathBuf> {
        let digest = Sha256::digest(format!("{service}:{account}").as_bytes());
        Ok(self.root.join(format!("{}.json", hex::encode(digest))))
    }

    fn service_hash(service: &str, account: &str) -> String {
        let digest = Sha256::digest(format!("{service}:{account}").as_bytes());
        hex::encode(digest)
    }
}

impl CredentialBackend for LocalEncryptedCredentialBackend {
    fn set_password(&self, service: &str, account: &str, value: &str) -> AppResult<()> {
        self.ensure_root()?;
        let key = self.master_key()?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut nonce_bytes = [0u8; LOCAL_NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: value.as_bytes(),
                    aad: service.as_bytes(),
                },
            )
            .map_err(|_| AppError::Credential("local credential encryption failed".into()))?;
        let mut payload = Vec::with_capacity(LOCAL_NONCE_LEN + ciphertext.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);
        let record = LocalCredentialRecord {
            version: 1,
            service_hash: Self::service_hash(service, account),
            ciphertext: B64.encode(payload),
        };
        let json = serde_json::to_vec(&record)?;
        write_private_file(&self.credential_path(service, account)?, &json)
    }

    fn get_password(&self, service: &str, account: &str) -> AppResult<Zeroizing<String>> {
        let path = self.credential_path(service, account)?;
        if !path.is_file() {
            return Err(missing_credential_error(service));
        }
        let record: LocalCredentialRecord = serde_json::from_slice(&fs::read(path)?)?;
        if record.version != 1 || record.service_hash != Self::service_hash(service, account) {
            return Err(AppError::Credential(
                "local credential record is corrupt".into(),
            ));
        }
        let encrypted = B64
            .decode(record.ciphertext)
            .map_err(|_| AppError::Credential("local credential record is corrupt".into()))?;
        if encrypted.len() < LOCAL_NONCE_LEN {
            return Err(AppError::Credential(
                "local credential record is corrupt".into(),
            ));
        }
        let (nonce_bytes, ciphertext) = encrypted.split_at(LOCAL_NONCE_LEN);
        let key = self.master_key()?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce_bytes),
                Payload {
                    msg: ciphertext,
                    aad: service.as_bytes(),
                },
            )
            .map_err(|_| AppError::Credential("local credential decryption failed".into()))?;
        let value = String::from_utf8(plaintext).map_err(|_| {
            AppError::Credential("local credential value is not valid UTF-8".into())
        })?;
        Ok(Zeroizing::new(value))
    }

    fn delete_password(&self, service: &str, account: &str) -> AppResult<()> {
        let path = self.credential_path(service, account)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err.into()),
        }
    }
}

fn local_backend() -> AppResult<LocalEncryptedCredentialBackend> {
    LocalEncryptedCredentialBackend::default()
}

fn set_private_dir_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)?;
    }
    Ok(())
}

/// LLM 厂商凭据 ID（与前端 `llmCredentialService` 一致）。
pub fn llm_credential_service(provider: &str) -> String {
    format!("iris.llm.{}", provider.trim())
}

#[cfg(test)]
fn mcp_credential_service(provider: &str) -> String {
    format!("iris.mcp.{}", provider.trim())
}

fn missing_credential_error(service: &str) -> AppError {
    AppError::Credential(format!("credential missing: {service}"))
}

fn is_missing_credential_error(error: &AppError) -> bool {
    matches!(error, AppError::Credential(message) if message.starts_with("credential missing: "))
}

fn checked_at_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn canonical_api_key_service(service: &str) -> AppResult<String> {
    let service = service.trim();
    validate_credential_service(service)?;
    Ok(service.to_string())
}

pub(crate) fn credential_status_dto(service: String, state: CredentialState) -> CredentialStatus {
    CredentialStatus {
        service,
        state,
        configured: state == CredentialState::Available,
        checked_at: checked_at_now(),
    }
}

pub(crate) fn credential_marker_key(service: &str) -> AppResult<String> {
    let canonical = canonical_api_key_service(service)?;
    Ok(format!("credential.configured.{canonical}"))
}

fn set_api_key_with_backend<B: CredentialBackend>(
    backend: &B,
    service: &str,
    value: &str,
) -> AppResult<CredentialStatus> {
    let canonical = canonical_api_key_service(service)?;
    if value.trim().is_empty() {
        return Err(AppError::msg("API Key 不能为空"));
    }

    backend.set_password(&canonical, SECRET_ACCOUNT, value)?;
    tracing::debug!("credential stored for {canonical}");
    Ok(credential_status_dto(canonical, CredentialState::Available))
}

fn get_runtime_secret_with_backend<B: CredentialBackend>(
    backend: &B,
    service: &str,
) -> AppResult<Zeroizing<String>> {
    let canonical = canonical_api_key_service(service)?;
    backend.get_password(&canonical, SECRET_ACCOUNT)
}

fn credential_status_with_backend<B: CredentialBackend>(
    backend: &B,
    service: &str,
) -> AppResult<CredentialStatus> {
    let canonical = canonical_api_key_service(service)?;
    match backend.get_password(&canonical, SECRET_ACCOUNT) {
        Ok(value) if !value.trim().is_empty() => {
            Ok(credential_status_dto(canonical, CredentialState::Available))
        }
        Ok(_) => Ok(credential_status_dto(canonical, CredentialState::Missing)),
        Err(err) if is_missing_credential_error(&err) => {
            Ok(credential_status_dto(canonical, CredentialState::Missing))
        }
        Err(err) => Err(err),
    }
}

fn credential_available_with_backend<B: CredentialBackend>(
    backend: &B,
    service: &str,
) -> AppResult<bool> {
    Ok(credential_status_with_backend(backend, service)?.configured)
}

fn delete_api_key_with_backend<B: CredentialBackend>(
    backend: &B,
    service: &str,
) -> AppResult<CredentialStatus> {
    let canonical = canonical_api_key_service(service)?;
    backend.delete_password(&canonical, SECRET_ACCOUNT)?;
    Ok(credential_status_dto(canonical, CredentialState::Missing))
}

/// Store or replace one LLM/MCP API key in Iris' local encrypted credential store.
pub fn set_api_key(service: &str, value: &str) -> AppResult<CredentialStatus> {
    set_api_key_with_backend(&local_backend()?, service, value)
}

/// Read one API key for runtime request assembly. The value must never be logged or persisted.
pub fn get_runtime_secret(service: &str) -> AppResult<Zeroizing<String>> {
    get_runtime_secret_with_backend(&local_backend()?, service)
}

/// Read the real local encrypted credential state for one LLM/MCP service.
pub fn credential_status(service: &str) -> AppResult<CredentialStatus> {
    credential_status_with_backend(&local_backend()?, service)
}

/// Check whether one LLM/MCP API key exists in Iris' local encrypted credential store.
pub fn credential_available(service: &str) -> AppResult<bool> {
    credential_available_with_backend(&local_backend()?, service)
}

/// Delete one LLM/MCP API key from Iris' local encrypted credential store.
pub fn delete_api_key(service: &str) -> AppResult<CredentialStatus> {
    delete_api_key_with_backend(&local_backend()?, service)
}

/// Store a generic application secret in Iris' local encrypted credential store.
pub fn set_secret(service: &str, value: &str) -> AppResult<()> {
    if service.trim().is_empty() {
        return Err(AppError::msg("凭据服务名不能为空"));
    }
    if value.trim().is_empty() {
        return Err(AppError::msg("凭据不能为空"));
    }
    local_backend()?.set_password(service.trim(), SECRET_ACCOUNT, value)?;
    tracing::debug!("credential stored for {}", service.trim());
    Ok(())
}

/// Read a generic application secret from Iris' local encrypted credential store.
pub fn get_secret(service: &str) -> AppResult<String> {
    if service.trim().is_empty() {
        return Err(AppError::msg("凭据服务名不能为空"));
    }
    Ok(local_backend()?
        .get_password(service.trim(), SECRET_ACCOUNT)?
        .to_string())
}

/// Check whether a generic application secret exists without exposing its value.
pub fn has_secret(service: &str) -> bool {
    get_secret(service).is_ok()
}

/// API keys no longer have an unlockable in-memory session; this is kept as a no-op IPC bridge.
pub fn credential_unlock_session() -> AppResult<()> {
    Ok(())
}

/// Clear runtime-only credential caches. API keys are not cached by the credential layer.
pub fn credential_lock_session() -> AppResult<()> {
    crate::cas::encryption::clear_cas_key_cache()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::sync::{LazyLock, Mutex};

    static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[derive(Default)]
    struct MemoryCredentialBackend {
        secrets: RefCell<BTreeMap<String, String>>,
        deleted_services: RefCell<Vec<String>>,
        get_calls: RefCell<Vec<String>>,
    }

    impl CredentialBackend for MemoryCredentialBackend {
        fn set_password(&self, service: &str, account: &str, value: &str) -> AppResult<()> {
            self.secrets
                .borrow_mut()
                .insert(format!("{service}:{account}"), value.to_string());
            Ok(())
        }

        fn get_password(&self, service: &str, account: &str) -> AppResult<Zeroizing<String>> {
            self.get_calls
                .borrow_mut()
                .push(format!("{service}:{account}"));
            self.secrets
                .borrow()
                .get(&format!("{service}:{account}"))
                .cloned()
                .map(Zeroizing::new)
                .ok_or_else(|| missing_credential_error(service))
        }

        fn delete_password(&self, service: &str, account: &str) -> AppResult<()> {
            self.secrets
                .borrow_mut()
                .remove(&format!("{service}:{account}"));
            self.deleted_services.borrow_mut().push(service.to_string());
            Ok(())
        }
    }

    #[test]
    fn llm_and_mcp_services_use_separate_canonical_targets() {
        assert_eq!(llm_credential_service("deepseek"), "iris.llm.deepseek");
        assert_eq!(mcp_credential_service("anysearch"), "iris.mcp.anysearch");
    }

    #[test]
    fn api_key_upsert_overwrites_one_service_without_touching_others() {
        let backend = MemoryCredentialBackend::default();

        set_api_key_with_backend(&backend, "iris.llm.deepseek", "old-key").expect("set old");
        set_api_key_with_backend(&backend, "iris.mcp.anysearch", "mcp-key").expect("set mcp");
        set_api_key_with_backend(&backend, "iris.llm.deepseek", "new-key").expect("overwrite llm");

        assert_eq!(
            get_runtime_secret_with_backend(&backend, "iris.llm.deepseek")
                .expect("get llm")
                .as_str(),
            "new-key"
        );
        assert_eq!(
            get_runtime_secret_with_backend(&backend, "iris.mcp.anysearch")
                .expect("get mcp")
                .as_str(),
            "mcp-key"
        );
    }

    #[test]
    fn api_key_set_does_not_read_secret_back_after_write() {
        let backend = MemoryCredentialBackend::default();

        set_api_key_with_backend(&backend, "iris.llm.deepseek", "new-key").expect("set key");

        assert!(backend.get_calls.borrow().is_empty());
    }

    #[test]
    fn credential_marker_key_is_non_secret_and_scoped_per_service() {
        assert_eq!(
            credential_marker_key("iris.llm.deepseek").expect("marker"),
            "credential.configured.iris.llm.deepseek"
        );
        assert_eq!(
            credential_marker_key("iris.mcp.anysearch").expect("marker"),
            "credential.configured.iris.mcp.anysearch"
        );
    }

    #[test]
    fn deleting_credential_removes_only_that_service() {
        let backend = MemoryCredentialBackend::default();
        set_api_key_with_backend(&backend, "iris.llm.deepseek", "deepseek-key").expect("set llm");
        set_api_key_with_backend(&backend, "iris.mcp.anysearch", "mcp-key").expect("set mcp");

        delete_api_key_with_backend(&backend, "iris.llm.deepseek").expect("delete llm");

        assert!(get_runtime_secret_with_backend(&backend, "iris.llm.deepseek").is_err());
        assert_eq!(
            get_runtime_secret_with_backend(&backend, "iris.mcp.anysearch")
                .expect("mcp remains")
                .as_str(),
            "mcp-key"
        );
    }

    #[test]
    fn local_encrypted_backend_roundtrips_without_plaintext_secret_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let backend = LocalEncryptedCredentialBackend::new_for_test(dir.path().join("credentials"));

        set_api_key_with_backend(&backend, "iris.llm.deepseek", "sk-secret-value")
            .expect("set local key");

        assert_eq!(
            get_runtime_secret_with_backend(&backend, "iris.llm.deepseek")
                .expect("read local key")
                .as_str(),
            "sk-secret-value"
        );

        let store_dump = std::fs::read_dir(dir.path().join("credentials"))
            .expect("credential dir")
            .filter_map(Result::ok)
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!store_dump.contains("sk-secret-value"));

        delete_api_key_with_backend(&backend, "iris.llm.deepseek").expect("delete local key");
        assert!(get_runtime_secret_with_backend(&backend, "iris.llm.deepseek").is_err());
    }

    #[test]
    fn credential_status_comes_from_local_encrypted_store() {
        let dir = tempfile::tempdir().expect("temp dir");
        let backend = LocalEncryptedCredentialBackend::new_for_test(dir.path().join("credentials"));

        assert_eq!(
            credential_status_with_backend(&backend, "iris.llm.deepseek")
                .expect("missing status")
                .state,
            CredentialState::Missing
        );

        set_api_key_with_backend(&backend, "iris.llm.deepseek", "real-key").expect("set key");

        assert_eq!(
            credential_status_with_backend(&backend, "iris.llm.deepseek")
                .expect("available status")
                .state,
            CredentialState::Available
        );

        delete_api_key_with_backend(&backend, "iris.llm.deepseek").expect("delete key");
        assert!(!credential_available_with_backend(&backend, "iris.llm.deepseek")
            .expect("available check"));
    }

    #[test]
    fn canonical_service_rejects_legacy_slashes() {
        assert!(canonical_api_key_service("iris/llm/deepseek").is_err());
    }

    #[test]
    fn credential_roundtrip() {
        let _guard = TEST_ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("temp dir");
        std::env::set_var("IRIS_DATA_DIR", dir.path());
        let id = format!("iris.llm.test_{}", uuid::Uuid::new_v4());
        set_api_key(&id, "test-secret-value").expect("set");
        assert_eq!(
            get_runtime_secret(&id).expect("get").as_str(),
            "test-secret-value"
        );
        delete_api_key(&id).expect("delete");
    }

    #[test]
    fn generic_secret_roundtrip() {
        let _guard = TEST_ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("temp dir");
        std::env::set_var("IRIS_DATA_DIR", dir.path());
        let id = format!("iris.test.{}", uuid::Uuid::new_v4());
        set_secret(&id, "test-secret-value").expect("set");
        assert!(has_secret(&id));
        assert_eq!(get_secret(&id).expect("get"), "test-secret-value");
        local_backend()
            .expect("backend")
            .delete_password(&id, SECRET_ACCOUNT)
            .expect("delete");
        assert!(!has_secret(&id));
    }
}
