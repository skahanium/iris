use crate::error::{AppError, AppResult};
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand::Rng;

pub const CSEF_MAGIC: &[u8; 4] = b"CSEF";
const NONCE_SIZE: usize = 12;

pub fn encrypt_cef(plaintext: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::msg("invalid key length for CEF encryption"))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rngs::OsRng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::msg(format!("CEF encryption failed: {e}")))?;

    let mut result = Vec::with_capacity(4 + NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(CSEF_MAGIC);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn decrypt_cef(data: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>> {
    if !has_csef_magic(data) {
        return Err(AppError::msg("not a CEF-encrypted file (missing magic)"));
    }
    if data.len() < 4 + NONCE_SIZE {
        return Err(AppError::msg("CEF data too short"));
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| AppError::msg("invalid key length for CEF decryption"))?;
    let nonce = Nonce::from_slice(&data[4..4 + NONCE_SIZE]);
    let ciphertext = &data[4 + NONCE_SIZE..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::msg(format!("CEF decryption failed: {e}")))
}

pub fn has_csef_magic(data: &[u8]) -> bool {
    data.len() >= 4 && &data[..4] == CSEF_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::needless_range_loop)]
    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for i in 0..32 {
            k[i] = i as u8;
        }
        k
    }

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let plain = b"Hello, classified world!";
        let key = test_key();
        let encrypted = encrypt_cef(plain, &key).unwrap();
        assert!(has_csef_magic(&encrypted));

        let decrypted = decrypt_cef(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let plain = b"top secret";
        let key1 = test_key();
        let mut key2 = test_key();
        key2[0] ^= 1;

        let encrypted = encrypt_cef(plain, &key1).unwrap();
        let result = decrypt_cef(&encrypted, &key2);
        assert!(result.is_err());
    }

    #[test]
    fn has_magic_detects_correctly() {
        let data = b"CSEFrest_of_data";
        assert!(has_csef_magic(data));

        let plain = b"just text";
        assert!(!has_csef_magic(plain));

        let short = b"CSE";
        assert!(!has_csef_magic(short));
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let key = test_key();
        let encrypted = encrypt_cef(b"", &key).unwrap();
        let decrypted = decrypt_cef(&encrypted, &key).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn corrupt_ciphertext_fails() {
        let plain = b"data";
        let key = test_key();
        let mut encrypted = encrypt_cef(plain, &key).unwrap();

        // Corrupt a byte in ciphertext
        let len = encrypted.len();
        encrypted[len - 5] ^= 0xFF;

        let result = decrypt_cef(&encrypted, &key);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_non_cef_data_fails() {
        let key = test_key();
        let result = decrypt_cef(b"plain text without magic", &key);
        assert!(result.is_err());
    }
}
