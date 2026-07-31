use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use zeroize::{Zeroize, Zeroizing};

use crate::credentials;
use crate::error::{AppError, AppResult};

#[cfg(debug_assertions)]
const CAS_KEY_SERVICE: &str = "iris.dev.cas_key";
#[cfg(not(debug_assertions))]
const CAS_KEY_SERVICE: &str = "iris.cas_key";
pub(crate) const NONCE_LEN: usize = 12;
pub(crate) const KEY_LEN: usize = 32;

/// 版本化 CAS 加密密钥环：`keys[version]` 即该版本对应的密钥。
///
/// blob 头记录写入时的版本号，读取按版本取密钥；被轮换的旧密钥永久保留在环中，
/// 因此显式轮换永远不会让历史快照变得不可读。版本号上限 255（头字段为 u8）。
#[derive(Clone)]
pub struct CasKeyRing {
    keys: Vec<[u8; KEY_LEN]>,
}

impl CasKeyRing {
    /// 由连续版本密钥构建环（版本 = 下标）。至少需要一把密钥。
    pub fn from_keys(keys: Vec<[u8; KEY_LEN]>) -> AppResult<Self> {
        if keys.is_empty() {
            return Err(AppError::msg("CAS key ring must contain at least one key"));
        }
        if keys.len() > u8::MAX as usize + 1 {
            return Err(AppError::msg(
                "CAS key ring exceeds the version-byte capacity (255)",
            ));
        }
        Ok(Self { keys })
    }

    /// 当前写入用版本（= 环中最后一版）。
    pub(crate) fn current_version(&self) -> u8 {
        (self.keys.len() - 1) as u8
    }

    /// 按版本取密钥；版本不存在返回 `None`。
    pub(crate) fn key_for(&self, version: u8) -> Option<[u8; KEY_LEN]> {
        self.keys.get(version as usize).copied()
    }

    /// 当前写入用密钥。
    pub(crate) fn current_key(&self) -> [u8; KEY_LEN] {
        self.keys[self.keys.len() - 1]
    }
}

#[derive(Clone)]
struct CachedCasKeyRing {
    ring: CasKeyRing,
}

impl Drop for CachedCasKeyRing {
    fn drop(&mut self) {
        for key in &mut self.ring.keys {
            key.zeroize();
        }
    }
}

static CAS_KEY_RING_CACHE: LazyLock<Mutex<Option<CachedCasKeyRing>>> =
    LazyLock::new(|| Mutex::new(None));

fn cache_lock() -> AppResult<std::sync::MutexGuard<'static, Option<CachedCasKeyRing>>> {
    CAS_KEY_RING_CACHE
        .lock()
        .map_err(|_| AppError::msg("CAS key ring cache lock error"))
}

pub(crate) fn clear_cas_key_cache() -> AppResult<()> {
    *cache_lock()? = None;
    Ok(())
}

fn cache_cas_ring(ring: CasKeyRing) -> AppResult<()> {
    *cache_lock()? = Some(CachedCasKeyRing { ring });
    Ok(())
}

fn ring_to_json(keys: &[[u8; KEY_LEN]]) -> String {
    let map: BTreeMap<String, String> = keys
        .iter()
        .enumerate()
        .map(|(version, key)| (version.to_string(), hex::encode(key)))
        .collect();
    serde_json::json!({
        "current": keys.len() - 1,
        "keys": map,
    })
    .to_string()
}

fn ring_from_json(encoded: &str) -> AppResult<Vec<[u8; KEY_LEN]>> {
    let value: Value = serde_json::from_str(encoded)
        .map_err(|e| AppError::msg(format!("corrupt CAS key ring: {e}")))?;
    let keys_obj = value
        .get("keys")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::msg("corrupt CAS key ring: missing keys"))?;
    let current = value
        .get("current")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::msg("corrupt CAS key ring: missing current"))?
        as usize;
    if current + 1 > u8::MAX as usize + 1 {
        return Err(AppError::msg("corrupt CAS key ring: version overflow"));
    }

    let mut slots: Vec<Option<[u8; KEY_LEN]>> = vec![None; current + 1];
    for (version_str, key_value) in keys_obj {
        let version: usize = version_str
            .parse()
            .map_err(|_| AppError::msg("corrupt CAS key ring: invalid version key"))?;
        if version > current {
            return Err(AppError::msg(
                "corrupt CAS key ring: version exceeds current",
            ));
        }
        let key_hex = key_value
            .as_str()
            .ok_or_else(|| AppError::msg("corrupt CAS key ring: non-string key"))?;
        let key_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
            hex::decode(key_hex)
                .map_err(|e| AppError::msg(format!("corrupt CAS key ring: {e}")))?,
        );
        if key_bytes.len() != KEY_LEN {
            return Err(AppError::msg("corrupt CAS key ring: incorrect key length"));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&key_bytes);
        slots[version] = Some(key);
    }
    for (version, slot) in slots.iter().enumerate() {
        if slot.is_none() {
            return Err(AppError::msg(format!(
                "corrupt CAS key ring: missing key for version {version}"
            )));
        }
    }
    Ok(slots.into_iter().flatten().collect())
}

/// 加载（必要时创建）版本化 CAS 密钥环。
///
/// 凭证记录已存在但不可解密/解析时**失败返回**，绝不静默生成新密钥——
/// 静默轮换会让既有快照永久不可读（历史事故的根因）。
pub fn load_or_create_cas_ring() -> AppResult<CasKeyRing> {
    if let Some(cached) = cache_lock()?.as_ref() {
        return Ok(cached.ring.clone());
    }

    match credentials::get_secret(CAS_KEY_SERVICE) {
        Ok(encoded) => {
            let keys = if encoded.starts_with('{') {
                ring_from_json(encoded.as_str())?
            } else {
                // legacy：纯 hex 单密钥记录，视为版本 0。
                let key_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
                    hex::decode(encoded.as_str())
                        .map_err(|e| AppError::msg(format!("corrupt CAS key: {e}")))?,
                );
                if key_bytes.len() != KEY_LEN {
                    return Err(AppError::msg("corrupt CAS key: incorrect length"));
                }
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&key_bytes);
                vec![key]
            };
            let ring = CasKeyRing::from_keys(keys)?;
            cache_cas_ring(ring.clone())?;
            Ok(ring)
        }
        Err(_) if credentials::has_secret(CAS_KEY_SERVICE) => Err(AppError::msg(
            "CAS key ring credential exists but cannot be decrypted; refusing to generate a new key (would make existing snapshots unreadable)",
        )),
        Err(_) => {
            let mut key = [0u8; KEY_LEN];
            OsRng.fill_bytes(&mut key);
            let ring = CasKeyRing::from_keys(vec![key])?;
            credentials::set_secret(CAS_KEY_SERVICE, &ring_to_json(&ring.keys))?;
            cache_cas_ring(ring.clone())?;
            tracing::info!("generated new CAS encryption key ring");
            Ok(ring)
        }
    }
}

/// 显式轮换：追加新密钥并写入环，旧密钥永久保留，历史 blob 保持可读。
pub fn rotate_cas_key() -> AppResult<CasKeyRing> {
    let current = load_or_create_cas_ring()?;
    let mut keys = current.keys.clone();
    if keys.len() > u8::MAX as usize {
        return Err(AppError::msg(
            "CAS key ring exhausted; retired keys cannot be dropped by design",
        ));
    }
    let mut new_key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut new_key);
    keys.push(new_key);
    let ring = CasKeyRing::from_keys(keys)?;
    credentials::set_secret(CAS_KEY_SERVICE, &ring_to_json(&ring.keys))?;
    *cache_lock()? = Some(CachedCasKeyRing { ring: ring.clone() });
    tracing::info!(
        "rotated CAS encryption key to version {}",
        ring.current_version()
    );
    Ok(ring)
}

/// Encrypt plaintext using AES-256-GCM.
/// Returns `nonce || ciphertext` (nonce is 12 bytes, prepended).
pub fn encrypt_blob(plaintext: &[u8], key: &[u8; KEY_LEN]) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::msg(format!("CAS encryption failed: {e}")))?;

    let mut result = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

/// Decrypt ciphertext produced by [`encrypt_blob`].
/// Expects `nonce (12 bytes) || ciphertext`.
pub fn decrypt_blob(encrypted: &[u8], key: &[u8; KEY_LEN]) -> AppResult<Vec<u8>> {
    if encrypted.len() < NONCE_LEN {
        return Err(AppError::msg("encrypted CAS blob too short"));
    }

    let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::msg(format!("CAS decryption failed: {e}")))
}

/// Check if the CAS encryption key exists (has been generated).
pub fn has_cas_key() -> bool {
    credentials::has_secret(CAS_KEY_SERVICE)
}

#[cfg(test)]
fn cache_cas_key_for_test(key: [u8; KEY_LEN]) -> AppResult<()> {
    cache_cas_ring(CasKeyRing::from_keys(vec![key])?)
}

#[cfg(test)]
fn cas_key_cached_for_test() -> AppResult<bool> {
    Ok(cache_lock()?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{LazyLock, Mutex};

    static CAS_TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct ScopedIrisEnvironment {
        data_dir: Option<OsString>,
        config_dir: Option<OsString>,
    }

    impl ScopedIrisEnvironment {
        fn new(data_dir: &std::path::Path, config_dir: &std::path::Path) -> Self {
            let environment = Self {
                data_dir: std::env::var_os("IRIS_DATA_DIR"),
                config_dir: std::env::var_os("IRIS_CONFIG_DIR"),
            };
            std::env::set_var("IRIS_DATA_DIR", data_dir);
            std::env::set_var("IRIS_CONFIG_DIR", config_dir);
            environment
        }
    }

    impl Drop for ScopedIrisEnvironment {
        fn drop(&mut self) {
            restore_env("IRIS_DATA_DIR", self.data_dir.take());
            restore_env("IRIS_CONFIG_DIR", self.config_dir.take());
        }
    }

    fn restore_env(key: &str, value: Option<OsString>) {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    fn test_env(dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        let data_dir = dir.path().join("data");
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&config_dir).unwrap();
        (data_dir, config_dir)
    }

    fn base64_encode(bytes: &[u8]) -> String {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        B64.encode(bytes)
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [0xAAu8; KEY_LEN];
        let plaintext = b"hello world test data for CAS encryption";

        let encrypted = encrypt_blob(plaintext, &key).unwrap();
        assert_ne!(encrypted, plaintext);
        assert!(encrypted.len() > plaintext.len());

        let decrypted = decrypt_blob(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key1 = [0xAAu8; KEY_LEN];
        let key2 = [0xBBu8; KEY_LEN];
        let plaintext = b"test";

        let encrypted = encrypt_blob(plaintext, &key1).unwrap();
        assert!(decrypt_blob(&encrypted, &key2).is_err());
    }

    #[test]
    fn nonce_is_unique_per_encryption() {
        let key = [0xAAu8; KEY_LEN];
        let a = encrypt_blob(b"test", &key).unwrap();
        let b = encrypt_blob(b"test", &key).unwrap();
        // Same plaintext, different nonces → different ciphertext
        assert_ne!(a, b);
    }

    #[test]
    fn decrypt_too_short_fails() {
        let key = [0xAAu8; KEY_LEN];
        assert!(decrypt_blob(&[1, 2, 3], &key).is_err());
    }

    #[test]
    fn legacy_hex_record_loads_as_ring_version_zero() {
        let _guard = CAS_TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (data_dir, config_dir) = test_env(&dir);
        let _environment = ScopedIrisEnvironment::new(&data_dir, &config_dir);

        clear_cas_key_cache().unwrap();
        credentials::set_secret(CAS_KEY_SERVICE, &hex::encode([0x11u8; KEY_LEN])).unwrap();

        let ring = load_or_create_cas_ring().unwrap();
        assert_eq!(ring.current_version(), 0);
        assert_eq!(ring.key_for(0), Some([0x11u8; KEY_LEN]));
        assert_eq!(ring.key_for(1), None);
    }

    #[test]
    fn missing_record_creates_ring_version_zero() {
        let _guard = CAS_TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (data_dir, config_dir) = test_env(&dir);
        let _environment = ScopedIrisEnvironment::new(&data_dir, &config_dir);

        clear_cas_key_cache().unwrap();

        let ring = load_or_create_cas_ring().unwrap();
        assert_eq!(ring.current_version(), 0);
        assert!(ring.key_for(0).is_some());
        assert!(credentials::has_secret(CAS_KEY_SERVICE));
        let stored = credentials::get_secret(CAS_KEY_SERVICE).unwrap();
        assert!(stored.starts_with('{'), "new record must use key-ring JSON");
    }

    #[test]
    fn rotate_keeps_old_keys_readable_and_persists_ring() {
        let _guard = CAS_TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (data_dir, config_dir) = test_env(&dir);
        let _environment = ScopedIrisEnvironment::new(&data_dir, &config_dir);

        clear_cas_key_cache().unwrap();
        let original = load_or_create_cas_ring().unwrap();
        let original_key = original.key_for(0).unwrap();

        let rotated = rotate_cas_key().unwrap();
        assert_eq!(rotated.current_version(), 1);
        assert_eq!(rotated.key_for(0), Some(original_key));
        assert_ne!(rotated.key_for(1), Some(original_key));
        assert_ne!(rotated.key_for(1), None);

        // 重新加载（清缓存）后环仍完整。
        clear_cas_key_cache().unwrap();
        let reloaded = load_or_create_cas_ring().unwrap();
        assert_eq!(reloaded.current_version(), 1);
        assert_eq!(reloaded.key_for(0), Some(original_key));
        assert_eq!(reloaded.key_for(1), rotated.key_for(1));
    }

    #[test]
    fn corrupt_credential_record_fails_hard_without_replacing() {
        let _guard = CAS_TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (data_dir, config_dir) = test_env(&dir);
        let _environment = ScopedIrisEnvironment::new(&data_dir, &config_dir);

        clear_cas_key_cache().unwrap();
        let original = load_or_create_cas_ring().unwrap();
        let original_key = original.key_for(0).unwrap();

        let service_hash = {
            use sha2::{Digest, Sha256};
            Sha256::digest(format!("{CAS_KEY_SERVICE}:api_key").as_bytes())
        };
        let record_path = data_dir
            .join("credentials")
            .join(format!("{}.json", hex::encode(service_hash)));
        let record_before = std::fs::read(&record_path).unwrap();

        // 用新的 master key 覆盖 config 目录的 master.key，模拟主密钥变更。
        let master_path = config_dir.join("master.key");
        let mut new_master = [0u8; 32];
        OsRng.fill_bytes(&mut new_master);
        std::fs::write(&master_path, base64_encode(&new_master)).unwrap();

        clear_cas_key_cache().unwrap();
        let result = load_or_create_cas_ring();
        assert!(
            result.is_err(),
            "corrupt record must fail instead of rotating"
        );

        // 记录文件必须原样保留，不能被新密钥覆盖。
        assert_eq!(
            std::fs::read(&record_path).unwrap(),
            record_before,
            "credential record must not be rewritten"
        );
        assert!(credentials::has_secret(CAS_KEY_SERVICE));
        let _ = original_key;
    }

    #[test]
    fn lock_session_clears_cached_cas_key() {
        cache_cas_key_for_test([0x11u8; KEY_LEN]).expect("cache key");

        crate::credentials::credential_lock_session().expect("lock session");

        assert!(!cas_key_cached_for_test().expect("cache state"));
    }
}
