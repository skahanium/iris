use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::app::AppState;
use crate::credentials;
use crate::error::AppResult;
use crate::security::ipc_policy::{validate_credential_service, validate_settings_key};

#[tauri::command]
pub fn settings_get(state: State<'_, Arc<AppState>>, key: String) -> AppResult<Option<Value>> {
    validate_settings_key(&key)?;
    state.db.with_conn(|conn| {
        let result: Result<String, _> =
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [&key], |r| {
                r.get(0)
            });
        match result {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

#[tauri::command]
pub fn settings_set(state: State<'_, Arc<AppState>>, key: String, value: Value) -> AppResult<()> {
    validate_settings_key(&key)?;
    let json = serde_json::to_string(&value)?;
    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![&key, json],
        )?;
        Ok(())
    })?;
    if key == "follow_system_proxy" {
        let follow = crate::network::parse_follow_system_proxy_setting(Some(&value));
        crate::network::set_follow_system_proxy(follow);
    }
    Ok(())
}

#[tauri::command]
pub fn settings_reset(state: State<'_, Arc<AppState>>, key: String) -> AppResult<()> {
    validate_settings_key(&key)?;
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM settings WHERE key = ?1", [&key])?;
        Ok(())
    })?;
    if key == "follow_system_proxy" {
        crate::network::set_follow_system_proxy(true);
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxyStatus {
    pub follow: bool,
    pub label: String,
}

#[tauri::command]
pub fn network_proxy_status() -> NetworkProxyStatus {
    NetworkProxyStatus {
        follow: crate::network::follow_system_proxy(),
        label: crate::network::proxy_status_label(),
    }
}

#[tauri::command]
pub fn credential_set(
    state: State<'_, Arc<AppState>>,
    service: String,
    value: String,
) -> AppResult<credentials::CredentialStatus> {
    validate_credential_service(&service)?;
    let status = credentials::set_api_key(&service, &value)?;
    set_credential_marker(&state.db, &service, true)?;
    Ok(status)
}

#[tauri::command]
pub fn credential_has(state: State<'_, Arc<AppState>>, service: String) -> AppResult<bool> {
    validate_credential_service(&service)?;
    credential_available_for_runtime(&state.db, &service)
}

#[tauri::command]
pub fn credential_status(
    state: State<'_, Arc<AppState>>,
    service: String,
) -> AppResult<credentials::CredentialStatus> {
    validate_credential_service(&service)?;
    let status = credentials::credential_status(&service)?;
    set_credential_marker(&state.db, &service, status.configured)?;
    Ok(status)
}

#[tauri::command]
pub fn credential_delete(
    state: State<'_, Arc<AppState>>,
    service: String,
) -> AppResult<credentials::CredentialStatus> {
    validate_credential_service(&service)?;
    let status = credentials::delete_api_key(&service)?;
    set_credential_marker(&state.db, &service, false)?;
    Ok(status)
}

#[tauri::command]
pub fn credential_lock_session() -> AppResult<()> {
    credentials::credential_lock_session()
}

pub(crate) fn credential_available_for_runtime(
    db: &crate::storage::db::Database,
    service: &str,
) -> AppResult<bool> {
    let available = credentials::credential_available(service)?;
    set_credential_marker(db, service, available)?;
    Ok(available)
}

pub(crate) fn set_credential_marker(
    db: &crate::storage::db::Database,
    service: &str,
    configured: bool,
) -> AppResult<()> {
    let key = credentials::credential_marker_key(service)?;
    db.with_conn(|conn| {
        if configured {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, "true"],
            )?;
        } else {
            conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        }
        Ok(())
    })
}
