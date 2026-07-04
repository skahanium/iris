use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use std::sync::{LazyLock, Mutex};
use zeroize::Zeroize;

use crate::credentials;
use crate::error::{AppError, AppResult};

#[cfg(debug_assertions)]
const CAS_KEY_SERVICE: &str = "iris.dev.cas_key";
#[cfg(not(debug_assertions))]
const CAS_KEY_SERVICE: &str = "iris.cas_key";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

#[derive(Clone)]
struct CachedCasKey {
    key: [u8; KEY_LEN],
}

impl Drop for CachedCasKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

static CAS_KEY_CACHE: LazyLock<Mutex<Option<CachedCasKey>>> = LazyLock::new(|| Mutex::new(None));

fn cache_lock() -> AppResult<std::sync::MutexGuard<'static, Option<CachedCasKey>>> {
    CAS_KEY_CACHE
        .lock()
        .map_err(|_| AppError::msg("CAS key cache lock error"))
}

pub(crate) fn clear_cas_key_cache() -> AppResult<()> {
    *cache_lock()? = None;
    Ok(())
}

fn cache_cas_key(key: [u8; KEY_LEN]) -> AppResult<()> {
    *cache_lock()? = Some(CachedCasKey { key });
    Ok(())
}

/// Get or generate the CAS encryption key from the OS credential store.
/// The key is derived once on first use and persisted to the keychain.
pub fn get_or_create_cas_key() -> AppResult<[u8; KEY_LEN]> {
    if let Some(cached) = cache_lock()?.as_ref() {
        return Ok(cached.key);
    }

    match credentials::get_secret(CAS_KEY_SERVICE) {
        Ok(hex_key) => {
            let key_bytes = hex::decode(&hex_key)
                .map_err(|e| AppError::msg(format!("corrupt CAS key: {e}")))?;
            let mut key = [0u8; KEY_LEN];
            if key_bytes.len() != KEY_LEN {
                return Err(AppError::msg("corrupt CAS key: incorrect length"));
            }
            key.copy_from_slice(&key_bytes);
            cache_cas_key(key)?;
            Ok(key)
        }
        Err(_) => {
            let mut key = [0u8; KEY_LEN];
            OsRng.fill_bytes(&mut key);
            let hex_key = hex::encode(key);
            credentials::set_secret(CAS_KEY_SERVICE, &hex_key)?;
            cache_cas_key(key)?;
            tracing::info!("generated new CAS encryption key");
            Ok(key)
        }
    }
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
    cache_cas_key(key)
}

#[cfg(test)]
fn cas_key_cached_for_test() -> AppResult<bool> {
    Ok(cache_lock()?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn lock_session_clears_cached_cas_key() {
        cache_cas_key_for_test([0x11u8; KEY_LEN]).expect("cache key");

        crate::credentials::credential_lock_session().expect("lock session");

        assert!(!cas_key_cached_for_test().expect("cache state"));
    }
}
