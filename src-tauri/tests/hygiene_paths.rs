use std::fs;
use std::time::{Duration, SystemTime};

#[cfg(target_os = "macos")]
use std::env;
#[cfg(target_os = "macos")]
use std::ffi::OsString;
#[cfg(target_os = "macos")]
use std::sync::{LazyLock, Mutex};

use iris_lib::hygiene::{cleanup_once, CleanupConfig};
#[cfg(target_os = "macos")]
use iris_lib::paths::prepare_iris_paths;
use iris_lib::paths::{resolve_iris_paths, IrisPathEnv};

#[cfg(target_os = "macos")]
const PREPARED_PATH_ENV_KEYS: &[&str] = &[
    "IRIS_HOME",
    "IRIS_DATA_DIR",
    "IRIS_CACHE_DIR",
    "IRIS_TEMP_DIR",
    "IRIS_GLOBAL_SKILLS_DIR",
    "ORT_CACHE_DIR",
    "HF_HOME",
    "HF_HUB_CACHE",
    "XDG_CACHE_HOME",
    "TMPDIR",
    "TEMP",
    "TMP",
];

#[cfg(target_os = "macos")]
static PATH_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[cfg(target_os = "macos")]
struct RestoredPathEnvironment(Vec<(&'static str, Option<OsString>)>);

#[cfg(target_os = "macos")]
impl RestoredPathEnvironment {
    fn capture() -> Self {
        Self(
            PREPARED_PATH_ENV_KEYS
                .iter()
                .map(|key| (*key, env::var_os(key)))
                .collect(),
        )
    }
}

#[cfg(target_os = "macos")]
impl Drop for RestoredPathEnvironment {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
    }
}

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

#[cfg(target_os = "macos")]
#[test]
fn resolve_iris_paths_uses_application_support_for_macos_app_bundles() {
    let dir = tempfile::tempdir().unwrap();
    let executable_dir = dir.path().join("Iris.app").join("Contents").join("MacOS");
    fs::create_dir_all(&executable_dir).unwrap();
    let executable = executable_dir.join("iris");
    fs::write(&executable, "binary").unwrap();
    let app_data_dir = dir
        .path()
        .join("Application Support")
        .join("com.iris.notes");

    let paths = resolve_iris_paths(IrisPathEnv {
        iris_home: None,
        iris_data_dir: None,
        iris_cache_dir: None,
        iris_temp_dir: None,
        iris_global_skills_dir: None,
        current_exe: Some(executable),
        allow_system_data_dir: false,
        tauri_app_data_dir: Some(app_data_dir.clone()),
    })
    .unwrap();

    assert_eq!(paths.home_dir, app_data_dir);
    assert_eq!(paths.data_dir, paths.home_dir.join("app-data"));
    assert_eq!(paths.cache_dir, paths.home_dir.join("cache"));
    assert_eq!(paths.temp_dir, paths.home_dir.join("tmp"));
    assert_eq!(paths.global_skills_dir, paths.home_dir.join("skills"));
    assert!(!paths.cache_dir.starts_with(&executable_dir));
    assert!(!paths.cache_dir.join("updates").starts_with(&executable_dir));
    assert!(!paths.temp_dir.starts_with(&executable_dir));
}

#[cfg(target_os = "macos")]
#[test]
fn prepare_iris_paths_sets_macos_bundle_temporary_directories_outside_the_app() {
    let _lock = PATH_ENV_LOCK.lock().unwrap();
    let _restore_environment = RestoredPathEnvironment::capture();
    let dir = tempfile::tempdir().unwrap();
    let executable_dir = dir.path().join("Iris.app").join("Contents").join("MacOS");
    fs::create_dir_all(&executable_dir).unwrap();
    let executable = executable_dir.join("iris");
    fs::write(&executable, "binary").unwrap();
    let app_data_dir = dir
        .path()
        .join("Application Support")
        .join("com.iris.notes");

    let paths = resolve_iris_paths(IrisPathEnv {
        iris_home: None,
        iris_data_dir: None,
        iris_cache_dir: None,
        iris_temp_dir: None,
        iris_global_skills_dir: None,
        current_exe: Some(executable),
        allow_system_data_dir: false,
        tauri_app_data_dir: Some(app_data_dir.clone()),
    })
    .unwrap();

    prepare_iris_paths(&paths).unwrap();

    assert_eq!(
        env::var_os("IRIS_CACHE_DIR"),
        Some(paths.cache_dir.clone().into())
    );
    for key in ["TMPDIR", "TEMP", "TMP"] {
        let temp_dir = std::path::PathBuf::from(env::var_os(key).unwrap());
        assert_eq!(temp_dir, paths.temp_dir);
        assert!(!temp_dir.starts_with(&executable_dir));
    }
    assert!(paths.cache_dir.join("updates").starts_with(app_data_dir));
}

#[cfg(target_os = "macos")]
#[test]
fn resolve_iris_paths_rejects_macos_app_bundles_without_application_support() {
    let dir = tempfile::tempdir().unwrap();
    let executable_dir = dir.path().join("Iris.app").join("Contents").join("MacOS");
    fs::create_dir_all(&executable_dir).unwrap();
    let executable = executable_dir.join("iris");
    fs::write(&executable, "binary").unwrap();

    let error = resolve_iris_paths(IrisPathEnv {
        iris_home: None,
        iris_data_dir: None,
        iris_cache_dir: None,
        iris_temp_dir: None,
        iris_global_skills_dir: None,
        current_exe: Some(executable),
        allow_system_data_dir: false,
        tauri_app_data_dir: None,
    })
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("macOS application bundle requires an application data directory"));
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

#[test]
fn cleanup_once_overwrites_expired_temp_files_before_removing_them() {
    let dir = tempfile::tempdir().unwrap();
    let temp_root = dir.path().join("tmp");
    fs::create_dir_all(&temp_root).unwrap();

    let expired_temp = temp_root.join("expired.tmp");
    let linked_copy = dir.path().join("linked-copy");
    let sensitive_content = b"sensitive temporary payload";
    fs::write(&expired_temp, sensitive_content).unwrap();
    fs::hard_link(&expired_temp, &linked_copy).unwrap();

    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 24 * 60 * 60);
    let old = now - Duration::from_secs(8 * 24 * 60 * 60);
    let report = cleanup_once(CleanupConfig {
        temp_dirs: vec![temp_root],
        cache_dirs: Vec::new(),
        now,
        temp_max_age: Duration::from_secs(7 * 24 * 60 * 60),
        cache_max_age: Duration::from_secs(30 * 24 * 60 * 60),
        modified_time: Box::new(move |_| old),
    })
    .unwrap();

    assert_eq!(report.deleted_files, 1);
    assert!(!expired_temp.exists());
    assert_eq!(
        fs::read(&linked_copy).unwrap(),
        vec![0; sensitive_content.len()]
    );
}
