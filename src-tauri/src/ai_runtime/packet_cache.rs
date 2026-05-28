//! 证据包 LRU 缓存
//!
//! 避免高频查询场景下的重复计算

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ai_runtime::ContextPacket;

/// 缓存条目
struct CacheEntry {
    packets: Vec<ContextPacket>,
    created_at: Instant,
    access_count: u32,
}

/// 证据包 LRU 缓存
pub struct PacketCache {
    cache: HashMap<String, CacheEntry>,
    max_entries: usize,
    ttl: Duration,
}

impl PacketCache {
    pub fn new(max_entries: usize, ttl_seconds: u64) -> Self {
        Self {
            cache: HashMap::new(),
            max_entries,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// 获取缓存的证据包
    pub fn get(&mut self, query_hash: &str) -> Option<Vec<ContextPacket>> {
        if let Some(entry) = self.cache.get_mut(query_hash) {
            if entry.created_at.elapsed() < self.ttl {
                entry.access_count += 1;
                return Some(entry.packets.clone());
            } else {
                self.cache.remove(query_hash);
            }
        }
        None
    }

    /// 存储证据包到缓存
    pub fn insert(&mut self, query_hash: String, packets: Vec<ContextPacket>) {
        if self.cache.len() >= self.max_entries {
            if let Some(oldest_key) = self.find_oldest_entry() {
                self.cache.remove(&oldest_key);
            }
        }

        self.cache.insert(
            query_hash,
            CacheEntry {
                packets,
                created_at: Instant::now(),
                access_count: 1,
            },
        );
    }

    /// 清除过期条目
    pub fn cleanup(&mut self) {
        self.cache
            .retain(|_, entry| entry.created_at.elapsed() < self.ttl);
    }

    fn find_oldest_entry(&self) -> Option<String> {
        self.cache
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(key, _)| key.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_get() {
        let mut cache = PacketCache::new(100, 300);
        let packets = vec![];
        cache.insert("test".to_string(), packets.clone());
        assert!(cache.get("test").is_some());
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_cache_ttl() {
        let mut cache = PacketCache::new(100, 0); // 0 秒 TTL
        cache.insert("test".to_string(), vec![]);
        std::thread::sleep(Duration::from_millis(10));
        assert!(cache.get("test").is_none());
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = PacketCache::new(2, 300);
        cache.insert("a".to_string(), vec![]);
        cache.insert("b".to_string(), vec![]);
        cache.insert("c".to_string(), vec![]); // should evict oldest ("a")
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn test_cache_cleanup() {
        let mut cache = PacketCache::new(100, 0);
        cache.insert("expired".to_string(), vec![]);
        std::thread::sleep(Duration::from_millis(10));
        cache.cleanup();
        assert!(cache.get("expired").is_none());
    }
}
