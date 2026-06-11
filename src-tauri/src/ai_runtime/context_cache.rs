use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextAssemblyCacheKey {
    scene: AiScene,
    note_path: Option<String>,
    query: String,
    scope_json: String,
    strategy: String,
    input_budget: u32,
}

impl ContextAssemblyCacheKey {
    pub fn new(
        scene: AiScene,
        note_path: Option<&str>,
        query: &str,
        scope_json: &str,
        strategy: &str,
        input_budget: u32,
    ) -> Self {
        Self {
            scene,
            note_path: note_path.map(str::to_string),
            query: query.to_string(),
            scope_json: scope_json.to_string(),
            strategy: strategy.to_string(),
            input_budget,
        }
    }
}

#[derive(Debug, Clone)]
struct ContextAssemblyCacheEntry {
    packets: Vec<ContextPacket>,
    status: ContextStatus,
    inserted_at: Instant,
    last_accessed: Instant,
}

#[derive(Debug)]
pub struct ContextAssemblyCache {
    max_entries: usize,
    ttl: Duration,
    entries: HashMap<ContextAssemblyCacheKey, ContextAssemblyCacheEntry>,
}

impl ContextAssemblyCache {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            max_entries,
            ttl: Duration::from_secs(ttl_secs),
            entries: HashMap::new(),
        }
    }

    pub fn get(
        &mut self,
        key: &ContextAssemblyCacheKey,
    ) -> Option<(Vec<ContextPacket>, ContextStatus)> {
        let now = Instant::now();
        let entry = self.entries.get_mut(key)?;
        if now.duration_since(entry.inserted_at) > self.ttl {
            self.entries.remove(key);
            return None;
        }
        entry.last_accessed = now;
        Some((entry.packets.clone(), entry.status.clone()))
    }

    pub fn insert(
        &mut self,
        key: ContextAssemblyCacheKey,
        packets: Vec<ContextPacket>,
        status: ContextStatus,
    ) {
        if self.max_entries == 0 {
            return;
        }
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&key) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_accessed)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        let now = Instant::now();
        self.entries.insert(
            key,
            ContextAssemblyCacheEntry {
                packets,
                status,
                inserted_at: now,
                last_accessed: now,
            },
        );
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
