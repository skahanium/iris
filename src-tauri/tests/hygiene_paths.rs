use std::fs;
use std::time::{Duration, SystemTime};

use iris_lib::hygiene::{cleanup_once, CleanupConfig};
use iris_lib::paths::{resolve_iris_paths, IrisPathEnv};

#[test]
fn resolve_iris_paths_prefers_explicit_iris_environment() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let data = dir.path().join("data");
    let cache = dir.path().join("cache");
    let temp = dir.path().join("tmp");
    let skills = dir.path().join("skills");

    let paths = resolve_iris_paths(IrisPathEnv {
        iris_home: Some(home.clone()),
        iris_data_dir: Some(data.clone()),
        iris_cache_dir: Some(cache.clone()),
        iris_temp_dir: Some(temp.clone()),
        iris_global_skills_dir: Some(skills.clone()),
        current_exe: None,
        allow_system_data_dir: false,
        tauri_app_data_dir: Some(dir.path().join("system-app-data")),
    })
    .unwrap();

    assert_eq!(paths.home_dir, home);
    assert_eq!(paths.data_dir, data);
    assert_eq!(paths.cache_dir, cache);
    assert_eq!(paths.temp_dir, temp);
    assert_eq!(paths.global_skills_dir, skills);
}

#[test]
fn resolve_iris_paths_uses_executable_portable_home_before_system_app_data() {
    let dir = tempfile::tempdir().unwrap();
    let exe_dir = dir.path().join("portable");
    fs::create_dir_all(&exe_dir).unwrap();
    let exe = exe_dir.join(if cfg!(windows) { "iris.exe" } else { "iris" });
    fs::write(&exe, "binary").unwrap();

    let paths = resolve_iris_paths(IrisPathEnv {
        iris_home: None,
        iris_data_dir: None,
        iris_cache_dir: None,
        iris_temp_dir: None,
        iris_global_skills_dir: None,
        current_exe: Some(exe),
        allow_system_data_dir: false,
        tauri_app_data_dir: Some(dir.path().join("system-app-data")),
    })
    .unwrap();

    assert_eq!(paths.home_dir, exe_dir.join(".iris"));
    assert_eq!(paths.data_dir, exe_dir.join(".iris").join("app-data"));
    assert_eq!(paths.cache_dir, exe_dir.join(".iris").join("cache"));
    assert_eq!(paths.temp_dir, exe_dir.join(".iris").join("tmp"));
    assert_eq!(
        paths.global_skills_dir,
        exe_dir.join(".iris").join("skills")
    );
}

#[test]
fn cleanup_once_removes_only_expired_whitelisted_cache_and_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let temp_root = dir.path().join("tmp");
    let cache_root = dir.path().join("cache");
    let data_root = dir.path().join("app-data");
    fs::create_dir_all(&temp_root).unwrap();
    fs::create_dir_all(cache_root.join("downloads")).unwrap();
    fs::create_dir_all(&data_root).unwrap();

    let old_temp = temp_root.join("old.tmp");
    let fresh_temp = temp_root.join("fresh.tmp");
    let old_cache = cache_root.join("downloads").join("old.part");
    let database = data_root.join("iris.db");
    let note = dir.path().join("note.md");

    fs::write(&old_temp, "old temp").unwrap();
    fs::write(&fresh_temp, "fresh temp").unwrap();
    fs::write(&old_cache, "old cache").unwrap();
    fs::write(&database, "database").unwrap();
    fs::write(&note, "# note").unwrap();

    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 24 * 60 * 60);
    let old = now - Duration::from_secs(40 * 24 * 60 * 60);

    let report = cleanup_once(CleanupConfig {
        temp_dirs: vec![temp_root.clone()],
        cache_dirs: vec![cache_root.clone()],
        now,
        temp_max_age: Duration::from_secs(7 * 24 * 60 * 60),
        cache_max_age: Duration::from_secs(30 * 24 * 60 * 60),
        modified_time: Box::new(move |path: &std::path::Path| {
            if path.ends_with("old.tmp") || path.ends_with("old.part") {
                old
            } else {
                now
            }
        }),
    })
    .unwrap();

    assert_eq!(report.deleted_files, 2);
    assert!(!old_temp.exists());
    assert!(!old_cache.exists());
    assert!(fresh_temp.exists());
    assert!(database.exists());
    assert!(note.exists());
}
