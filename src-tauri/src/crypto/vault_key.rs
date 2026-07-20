#![allow(unused_imports)] // Task 3 计划指定导入（PasswordVerifier / PasswordHash / RwLock）

use crate::error::{AppError, AppResult};
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use rand::Rng;
use std::fs;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use zeroize::{Zeroize, ZeroizeOnDrop};

const VAULT_CONFIG_FILENAME: &str = "vault.json";
const VERIFY_PLAINTEXT: &[u8] = b"iris-classified-vault-verify";

#[derive(serde::Serialize, serde::Deserialize)]
struct VaultConfig {
    version: u32,
    salt: String,
    verification: String,
}

#[derive(Debug, Clone, Zeroize, ZeroizeOnDrop)]
struct KeyBytes([u8; 32]);

#[derive(Debug)]
pub struct VaultKey {
    key: Option<KeyBytes>,
}

#[allow(clippy::new_without_default)]
impl VaultKey {
    pub fn new() -> Self {
        Self { key: None }
    }

    fn config_path(vault_path: &Path) -> std::path::PathBuf {
        vault_path
            .join(".iris")
            .join(VAULT_CONFIG_FILENAME)
    }

    fn derive_key(password: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
        let argon2 = Argon2::default();
        let salt_string = SaltString::encode_b64(salt)
            .map_err(|e| AppError::msg(format!("salt encoding failed: {e}")))?;
        let hash = argon2
            .hash_password(password.as_bytes(), &salt_string)
            .map_err(|e| AppError::msg(format!("key derivation failed: {e}")))?;

        let mut key = [0u8; 32];
        let hash_output = hash.hash.unwrap();
        let hash_bytes = hash_output.as_bytes();
        let len = hash_bytes.len().min(32);
        key[..len].copy_from_slice(&hash_bytes[..len]);
        Ok(key)
    }

    fn encrypt_verify(plaintext: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|_| AppError::msg("invalid key length"))?;
        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| AppError::msg(format!("encryption failed: {e}")))?;

        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    fn decrypt_verify(data: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>> {
        use aes_gcm::aead::Aead;
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

        if data.len() < 12 {
            return Err(AppError::msg("verification data too short"));
        }

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|_| AppError::msg("invalid key length"))?;
        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AppError::msg(format!("verification decryption failed: {e}")))
    }

    pub fn is_initialized(vault_path: &Path) -> bool {
        Self::config_path(vault_path).is_file()
    }

    /// Verify that the persisted classified-vault configuration is readable and well formed.
    pub(crate) fn config_accessible(vault_path: &Path) -> AppResult<()> {
        let json = fs::read_to_string(Self::config_path(vault_path))
            .map_err(|_| AppError::msg("保险库配置文件不可访问"))?;
        let config: VaultConfig = serde_json::from_str(&json)
            .map_err(|_| AppError::msg("保险库配置文件已损坏"))?;
        if config.version != 1
            || hex::decode(config.salt).is_err()
            || hex::decode(config.verification).is_err()
        {
            return Err(AppError::msg("保险库配置文件已损坏"));
        }
        Ok(())
    }

    pub fn setup(password: &str, vault_path: &Path) -> AppResult<()> {
        let iris_dir = vault_path.join(".iris");
        fs::create_dir_all(&iris_dir)?;

        let mut salt = [0u8; 32];
        rand::rngs::OsRng.fill(&mut salt);

        let key = Self::derive_key(password, &salt)?;
        let verification = Self::encrypt_verify(VERIFY_PLAINTEXT, &key)?;

        let config = VaultConfig {
            version: 1,
            #[allow(clippy::needless_borrows_for_generic_args)]
            salt: hex::encode(&salt),
            verification: hex::encode(&verification),
        };

        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::msg(format!("config serialization: {e}")))?;
        fs::write(Self::config_path(vault_path), json)?;

        let classified_dir = vault_path.join(".classified");
        fs::create_dir_all(&classified_dir)?;

        Ok(())
    }

    pub fn unlock(&mut self, password: &str, vault_path: &Path) -> AppResult<()> {
        let json = fs::read_to_string(Self::config_path(vault_path))
            .map_err(|e| AppError::msg(format!("无法读取保险库配置: {e}")))?;
        let config: VaultConfig = serde_json::from_str(&json)
            .map_err(|_| AppError::msg("保险库配置文件已损坏"))?;

        let salt = hex::decode(&config.salt)
            .map_err(|_| AppError::msg("保险库配置中 salt 无效"))?;
        let verification = hex::decode(&config.verification)
            .map_err(|_| AppError::msg("保险库配置中 verification 无效"))?;

        let key = Self::derive_key(password, &salt)?;

        match Self::decrypt_verify(&verification, &key) {
            Ok(pt) if pt == VERIFY_PLAINTEXT => {
                self.key = Some(KeyBytes(key));
                Ok(())
            }
            _ => Err(AppError::msg("密码不正确")),
        }
    }

    pub fn lock(&mut self) {
        if let Some(mut k) = self.key.take() {
            k.zeroize();
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    pub fn key(&self) -> AppResult<&[u8; 32]> {
        self.key
            .as_ref()
            .map(|k| &k.0)
            .ok_or_else(|| AppError::msg("保险库未解锁"))
    }

    #[cfg(test)]
    pub fn set_test_key(&mut self, key: [u8; 32]) {
        self.key = Some(KeyBytes(key));
    }
}

pub static VAULT_KEY: OnceLock<RwLock<VaultKey>> = OnceLock::new();

#[cfg(test)]
pub static VAULT_KEY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Initialize the process-wide vault key holder (call once at app startup).
pub fn init_vault_key() {
    let _ = VAULT_KEY.get_or_init(|| RwLock::new(VaultKey::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn setup_creates_config_and_classified_dir() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        VaultKey::setup("test-password", &vault).unwrap();

        let config_path = vault.join(".iris").join("vault.json");
        assert!(config_path.exists());
        assert!(vault.join(".classified").exists());
    }

    #[test]
    fn unlock_with_correct_password_succeeds() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        VaultKey::setup("my-secret", &vault).unwrap();

        let mut vk = VaultKey::new();
        vk.unlock("my-secret", &vault).unwrap();
        assert!(vk.is_unlocked());
    }

    #[test]
    fn unlock_with_wrong_password_fails() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        VaultKey::setup("correct", &vault).unwrap();

        let mut vk = VaultKey::new();
        let err = vk.unlock("wrong", &vault).unwrap_err();
        assert!(err.to_string().contains("密码不正确"));
        assert!(!vk.is_unlocked());
    }

    #[test]
    fn lock_clears_key() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        VaultKey::setup("test", &vault).unwrap();

        let mut vk = VaultKey::new();
        vk.unlock("test", &vault).unwrap();

        vk.lock();
        assert!(!vk.is_unlocked());
        assert!(vk.key().is_err());
    }

    #[test]
    fn derive_key_deterministic() {
        let salt = [0x42u8; 32];
        let k1 = VaultKey::derive_key("password", &salt).unwrap();
        let k2 = VaultKey::derive_key("password", &salt).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn is_initialized_correct() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        assert!(!VaultKey::is_initialized(&vault));
        VaultKey::setup("test", &vault).unwrap();
        assert!(VaultKey::is_initialized(&vault));
    }

    #[test]
    fn config_accessible_accepts_valid_config_and_rejects_corruption() {
        let dir = tempdir().unwrap();
        let vault = dir.path().join("vault");
        fs::create_dir_all(&vault).unwrap();

        assert!(VaultKey::config_accessible(&vault).is_err());
        VaultKey::setup("test", &vault).unwrap();

        assert!(VaultKey::config_accessible(&vault).is_ok());
        fs::write(vault.join(".iris").join(VAULT_CONFIG_FILENAME), "not json").unwrap();
        assert!(VaultKey::config_accessible(&vault).is_err());
    }
}
