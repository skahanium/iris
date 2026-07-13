use iris_lib::ai_runtime::context_cache::{ContextAssemblyCache, ContextAssemblyCacheKey};
use iris_lib::ai_runtime::ContextStatus;

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
        Some("notes/a.md"),
        "query",
        "{}",
        "balanced",
        4096,
        "profile-a",
    );

    cache.insert(key.clone(), vec![], status(12));
    assert_eq!(cache.get(&key).unwrap().1.total_tokens_estimate, 12);

    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(cache.get(&key).is_none());
}

#[test]
fn context_cache_evicts_lru_entry() {
    let mut cache = ContextAssemblyCache::new(2, 60);
    let a = ContextAssemblyCacheKey::new(None, "a", "{}", "fast", 1, "p");
    let b = ContextAssemblyCacheKey::new(None, "b", "{}", "fast", 1, "p");
    let c = ContextAssemblyCacheKey::new(None, "c", "{}", "fast", 1, "p");

    cache.insert(a.clone(), vec![], status(1));
    cache.insert(b.clone(), vec![], status(2));
    assert!(cache.get(&a).is_some());
    cache.insert(c.clone(), vec![], status(3));

    assert!(cache.get(&a).is_some());
    assert!(cache.get(&b).is_none());
    assert!(cache.get(&c).is_some());
}

#[test]
fn context_cache_key_separates_prompt_profiles() {
    let mut cache = ContextAssemblyCache::new(2, 60);
    let default_profile = ContextAssemblyCacheKey::new(None, "q", "{}", "fast", 1, "a");
    let strict_profile = ContextAssemblyCacheKey::new(None, "q", "{}", "fast", 1, "b");

    cache.insert(default_profile.clone(), vec![], status(11));

    assert_eq!(
        cache.get(&default_profile).unwrap().1.total_tokens_estimate,
        11
    );
    assert!(cache.get(&strict_profile).is_none());
}
