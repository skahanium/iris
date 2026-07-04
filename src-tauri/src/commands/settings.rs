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
            rusqlite::params![key, json],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn settings_reset(state: State<'_, Arc<AppState>>, key: String) -> AppResult<()> {
    validate_settings_key(&key)?;
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    })
}

#[tauri::command]
pub fn credential_set(
    state: State<'_, Arc<AppState>>,
    service: String,
    value: String,
) -> AppResult<()> {
    validate_credential_service(&service)?;
    credentials::set_api_key(&state.db, &service, &value)
}

#[tauri::command]
pub fn credential_has(state: State<'_, Arc<AppState>>, service: String) -> AppResult<bool> {
    validate_credential_service(&service)?;
    credentials::api_key_configured(&state.db, &service)
}

#[tauri::command]
pub fn credential_delete(state: State<'_, Arc<AppState>>, service: String) -> AppResult<()> {
    validate_credential_service(&service)?;
    credentials::delete_api_key(&state.db, &service)
}

#[tauri::command]
pub fn credential_unlock_session() -> AppResult<()> {
    credentials::credential_unlock_session()
}

#[tauri::command]
pub fn credential_lock_session() -> AppResult<()> {
    credentials::credential_lock_session()
}
