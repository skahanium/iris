use std::env;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrisPaths {
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub global_skills_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct IrisPathEnv {
    pub iris_home: Option<PathBuf>,
    pub iris_data_dir: Option<PathBuf>,
    pub iris_cache_dir: Option<PathBuf>,
    pub iris_temp_dir: Option<PathBuf>,
    pub iris_global_skills_dir: Option<PathBuf>,
    pub current_exe: Option<PathBuf>,
    pub allow_system_data_dir: bool,
    pub tauri_app_data_dir: Option<PathBuf>,
}

pub fn resolve_iris_paths(input: IrisPathEnv) -> AppResult<IrisPaths> {
    let home_dir = match clean_path(input.iris_home) {
        Some(path) => path,
        None => portable_home(input.current_exe.clone()).or_else(|| {
            input
                .allow_system_data_dir
                .then(|| input.tauri_app_data_dir.clone())
                .flatten()
        })
        .ok_or_else(|| {
            AppError::msg(
                "IRIS_HOME is not set and the executable directory could not be resolved; refusing to use a system data directory implicitly",
            )
        })?,
    };

    let paths = IrisPaths {
        data_dir: clean_path(input.iris_data_dir).unwrap_or_else(|| home_dir.join("app-data")),
        cache_dir: clean_path(input.iris_cache_dir).unwrap_or_else(|| home_dir.join("cache")),
        temp_dir: clean_path(input.iris_temp_dir).unwrap_or_else(|| home_dir.join("tmp")),
        global_skills_dir: clean_path(input.iris_global_skills_dir)
            .unwrap_or_else(|| home_dir.join("skills")),
        home_dir,
    };

    Ok(paths)
}

pub fn resolve_iris_paths_from_process(
    tauri_app_data_dir: Option<PathBuf>,
) -> AppResult<IrisPaths> {
    let allow_system_data_dir = env::var("IRIS_ALLOW_SYSTEM_DATA_DIR")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    resolve_iris_paths(IrisPathEnv {
        iris_home: env_path("IRIS_HOME"),
        iris_data_dir: env_path("IRIS_DATA_DIR"),
        iris_cache_dir: env_path("IRIS_CACHE_DIR"),
        iris_temp_dir: env_path("IRIS_TEMP_DIR"),
        iris_global_skills_dir: env_path("IRIS_GLOBAL_SKILLS_DIR"),
        current_exe: env::current_exe().ok(),
        allow_system_data_dir,
        tauri_app_data_dir,
    })
}

pub fn prepare_iris_paths(paths: &IrisPaths) -> AppResult<()> {
    for dir in [
        &paths.home_dir,
        &paths.data_dir,
        &paths.cache_dir,
        &paths.temp_dir,
        &paths.global_skills_dir,
        &paths.cache_dir.join("ort"),
        &paths.cache_dir.join("huggingface"),
        &paths.cache_dir.join("huggingface").join("hub"),
    ] {
        std::fs::create_dir_all(dir)?;
        assert_writable(dir)?;
    }

    set_env_path("IRIS_HOME", &paths.home_dir);
    set_env_path("IRIS_DATA_DIR", &paths.data_dir);
    set_env_path("IRIS_CACHE_DIR", &paths.cache_dir);
    set_env_path("IRIS_TEMP_DIR", &paths.temp_dir);
    set_env_path("IRIS_GLOBAL_SKILLS_DIR", &paths.global_skills_dir);
    set_env_path("ORT_CACHE_DIR", &paths.cache_dir.join("ort"));
    set_env_path("HF_HOME", &paths.cache_dir.join("huggingface"));
    set_env_path(
        "HF_HUB_CACHE",
        &paths.cache_dir.join("huggingface").join("hub"),
    );
    set_env_path("XDG_CACHE_HOME", &paths.cache_dir.join("xdg"));
    set_env_path("TMPDIR", &paths.temp_dir);
    set_env_path("TEMP", &paths.temp_dir);
    set_env_path("TMP", &paths.temp_dir);

    Ok(())
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key).and_then(|value| {
        let path = PathBuf::from(value);
        clean_path(Some(path))
    })
}

fn clean_path(path: Option<PathBuf>) -> Option<PathBuf> {
    path.filter(|value| !value.as_os_str().is_empty())
}

fn portable_home(current_exe: Option<PathBuf>) -> Option<PathBuf> {
    current_exe.and_then(|exe| exe.parent().map(|parent| parent.join(".iris")))
}

fn set_env_path(key: &str, value: &std::path::Path) {
    env::set_var(key, value.as_os_str());
}

fn assert_writable(dir: &std::path::Path) -> AppResult<()> {
    let probe = dir.join(".iris-write-test");
    std::fs::write(&probe, b"ok").map_err(|e| {
        AppError::msg(format!(
            "Iris directory is not writable: {} ({e})",
            dir.display()
        ))
    })?;
    std::fs::remove_file(&probe)?;
    Ok(())
}
