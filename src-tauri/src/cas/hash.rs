use sha2::{Digest, Sha256};

/// 统一的内容哈希计算接口
pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// 字符串内容哈希（便捷函数）
pub fn content_hash_str(content: &str) -> String {
    content_hash(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let content = "Hello, World!";
        let hash1 = content_hash_str(content);
        let hash2 = content_hash_str(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_different_content() {
        let hash1 = content_hash_str("Hello");
        let hash2 = content_hash_str("World");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_empty_content() {
        let hash = content_hash_str("");
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA-256 hex length
    }

    #[test]
    fn test_content_hash_binary_content() {
        let content = vec![0u8, 1, 2, 3, 255];
        let hash = content_hash(&content);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }
}
