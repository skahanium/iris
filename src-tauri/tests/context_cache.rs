use iris_lib::ai_runtime::context_cache::{ContextAssemblyCache, ContextAssemblyCacheKey};
use iris_lib::ai_runtime::{AiScene, ContextStatus};

fn status(tokens: usize) -> ContextStatus {
    ContextStatus {
        regulations_loaded: 0,
        model_essays_loaded: 0,
        anchors_loaded: 0,
        links_loaded: 0,
        total_tokens_estimate: tokens,
    }
}

#[test]
fn context_cache_hits_and_expires_by_ttl() {
    let mut cache = ContextAssemblyCache::new(2, 1);
    let key = ContextAssemblyCacheKey::new(
        AiScene::KnowledgeLookup,
        Some("notes/a.md"),
        "query",
        "{}",
        "balanced",
        4096,
    );

    cache.insert(key.clone(), vec![], status(12));
    assert_eq!(cache.get(&key).unwrap().1.total_tokens_estimate, 12);

    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(cache.get(&key).is_none());
}

#[test]
fn context_cache_evicts_lru_entry() {
    let mut cache = ContextAssemblyCache::new(2, 60);
    let a = ContextAssemblyCacheKey::new(AiScene::KnowledgeLookup, None, "a", "{}", "fast", 1);
    let b = ContextAssemblyCacheKey::new(AiScene::KnowledgeLookup, None, "b", "{}", "fast", 1);
    let c = ContextAssemblyCacheKey::new(AiScene::KnowledgeLookup, None, "c", "{}", "fast", 1);

    cache.insert(a.clone(), vec![], status(1));
    cache.insert(b.clone(), vec![], status(2));
    assert!(cache.get(&a).is_some());
    cache.insert(c.clone(), vec![], status(3));

    assert!(cache.get(&a).is_some());
    assert!(cache.get(&b).is_none());
    assert!(cache.get(&c).is_some());
}
