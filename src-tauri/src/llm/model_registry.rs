use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::ai_types::CapabilitySlot;
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Source of a model registry row.
pub enum ModelRegistrySource {
    BuiltIn,
    ProviderDiscovered,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kind of live validation that succeeded for a model.
pub enum ModelValidationKind {
    Text,
    Vision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Persisted model registry entry exposed to callers.
pub struct ModelRegistryEntry {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub source: ModelRegistrySource,
    pub stale: bool,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_refreshed_at: Option<String>,
    pub text_verified_at: Option<String>,
    pub vision_verified_at: Option<String>,
    pub user_confirmed_capabilities: Vec<CapabilitySlot>,
}

impl ModelRegistrySource {
    fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::ProviderDiscovered => "provider_discovered",
            Self::Manual => "manual",
        }
    }

    fn from_db(value: &str) -> AppResult<Self> {
        match value {
            "built_in" => Ok(Self::BuiltIn),
            "provider_discovered" => Ok(Self::ProviderDiscovered),
            "manual" => Ok(Self::Manual),
            _ => Err(AppError::msg(format!(
                "unknown model registry source: {value}"
            ))),
        }
    }
}

fn parse_capabilities(raw: &str) -> AppResult<Vec<CapabilitySlot>> {
    serde_json::from_str(raw).map_err(Into::into)
}

fn serialize_capabilities(capabilities: &[CapabilitySlot]) -> AppResult<String> {
    serde_json::to_string(capabilities).map_err(Into::into)
}

fn map_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelRegistryEntry> {
    let source_raw: String = row.get(3)?;
    let capabilities_raw: String = row.get(10)?;
    let source = ModelRegistrySource::from_db(&source_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let user_confirmed_capabilities = parse_capabilities(&capabilities_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(ModelRegistryEntry {
        provider_id: row.get(0)?,
        model_id: row.get(1)?,
        display_name: row.get(2)?,
        source,
        stale: row.get::<_, i64>(4)? != 0,
        first_seen_at: row.get(5)?,
        last_seen_at: row.get(6)?,
        last_refreshed_at: row.get(7)?,
        text_verified_at: row.get(8)?,
        vision_verified_at: row.get(9)?,
        user_confirmed_capabilities,
    })
}

/// Replace a provider discovery snapshot while preserving old rows as stale.
pub fn upsert_provider_discovered_models<I, S>(
    db: &Database,
    provider_id: &str,
    model_ids: I,
) -> AppResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut ids = Vec::new();
    for model_id in model_ids {
        let model_id = model_id.as_ref().trim();
        if !model_id.is_empty() {
            ids.push(model_id.to_string());
        }
    }

    db.with_conn(|conn| {
        conn.execute(
            "UPDATE llm_model_registry
             SET stale = 1, last_refreshed_at = datetime('now')
             WHERE provider_id = ?1 AND source = ?2",
            params![
                provider_id,
                ModelRegistrySource::ProviderDiscovered.as_str()
            ],
        )?;

        for model_id in ids {
            conn.execute(
                "INSERT INTO llm_model_registry
                 (provider_id, model_id, display_name, source, stale, first_seen_at, last_seen_at,
                  last_refreshed_at, user_confirmed_capabilities)
                 VALUES (?1, ?2, ?2, ?3, 0, datetime('now'), datetime('now'), datetime('now'), '[]')
                 ON CONFLICT(provider_id, model_id) DO UPDATE SET
                    display_name = excluded.display_name,
                    source = excluded.source,
                    stale = 0,
                    last_seen_at = datetime('now'),
                    last_refreshed_at = datetime('now')",
                params![
                    provider_id,
                    model_id,
                    ModelRegistrySource::ProviderDiscovered.as_str()
                ],
            )?;
        }

        Ok(())
    })
}

/// Delete all persisted registry rows for a provider.
pub fn delete_provider_entries(db: &Database, provider_id: &str) -> AppResult<usize> {
    db.with_conn(|conn| {
        Ok(conn.execute(
            "DELETE FROM llm_model_registry WHERE provider_id = ?1",
            params![provider_id],
        )?)
    })
}

/// List all model registry entries.
pub fn list_registry_entries(db: &Database) -> AppResult<Vec<ModelRegistryEntry>> {
    let sql = "SELECT provider_id, model_id, display_name, source, stale,
                     first_seen_at, last_seen_at, last_refreshed_at,
                     text_verified_at, vision_verified_at, user_confirmed_capabilities
              FROM llm_model_registry";
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(&format!("{sql} ORDER BY provider_id, model_id"))?;
        let rows = stmt.query_map([], map_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

/// Remove stale vision probe markers for catalog models known not to support vision.
pub fn clear_invalid_vision_validations(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let mut cleared = 0;
        for model in crate::llm::model_catalog::catalog()
            .iter()
            .filter(|model| !model.supports_vision)
        {
            cleared += conn.execute(
                "UPDATE llm_model_registry
                 SET vision_verified_at = NULL
                 WHERE provider_id = ?1
                   AND model_id = ?2
                   AND vision_verified_at IS NOT NULL",
                params![model.provider_id, model.id],
            )?;
        }
        Ok(cleared)
    })
}

/// Mark a model as user-confirmed for a capability slot.
pub fn confirm_capability(
    db: &Database,
    provider_id: &str,
    model_id: &str,
    slot: CapabilitySlot,
) -> AppResult<ModelRegistryEntry> {
    let entry = db.with_conn(|conn| {
        let existing: Option<String> = conn
            .query_row(
                "SELECT user_confirmed_capabilities
                 FROM llm_model_registry
                 WHERE provider_id = ?1 AND model_id = ?2",
                params![provider_id, model_id],
                |row| row.get(0),
            )
            .optional()?;

        let mut capabilities = existing
            .as_deref()
            .map(parse_capabilities)
            .transpose()?
            .unwrap_or_default();
        if !capabilities.contains(&slot) {
            capabilities.push(slot);
        }
        let capabilities_json = serialize_capabilities(&capabilities)?;

        conn.execute(
            "INSERT INTO llm_model_registry
             (provider_id, model_id, display_name, source, stale, first_seen_at, last_seen_at,
              last_refreshed_at, text_verified_at, vision_verified_at, user_confirmed_capabilities)
             VALUES (?1, ?2, ?2, ?3, 0, datetime('now'), datetime('now'), datetime('now'),
                     NULL,
                     NULL,
                     ?4)
             ON CONFLICT(provider_id, model_id) DO UPDATE SET
                user_confirmed_capabilities = excluded.user_confirmed_capabilities",
            params![
                provider_id,
                model_id,
                ModelRegistrySource::Manual.as_str(),
                capabilities_json,
            ],
        )?;

        let entry = conn.query_row(
            "SELECT provider_id, model_id, display_name, source, stale,
                    first_seen_at, last_seen_at, last_refreshed_at,
                    text_verified_at, vision_verified_at, user_confirmed_capabilities
             FROM llm_model_registry
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model_id],
            map_entry,
        )?;
        Ok(entry)
    })?;
    Ok(entry)
}

/// Mark a model as successfully validated by a live probe.
pub fn mark_model_validated(
    db: &Database,
    provider_id: &str,
    model_id: &str,
    kind: ModelValidationKind,
) -> AppResult<ModelRegistryEntry> {
    let entry = db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO llm_model_registry
             (provider_id, model_id, display_name, source, stale, first_seen_at, last_seen_at,
              last_refreshed_at, text_verified_at, vision_verified_at, user_confirmed_capabilities)
             VALUES (?1, ?2, ?2, ?3, 0, datetime('now'), datetime('now'), datetime('now'),
                     CASE WHEN ?4 = 1 THEN datetime('now') ELSE NULL END,
                     CASE WHEN ?4 = 2 THEN datetime('now') ELSE NULL END,
                     '[]')
             ON CONFLICT(provider_id, model_id) DO UPDATE SET
                text_verified_at = CASE
                    WHEN ?4 = 1 THEN datetime('now')
                    ELSE llm_model_registry.text_verified_at
                END,
                vision_verified_at = CASE
                    WHEN ?4 = 2 THEN datetime('now')
                    ELSE llm_model_registry.vision_verified_at
                END,
                last_seen_at = datetime('now')",
            params![
                provider_id,
                model_id,
                ModelRegistrySource::Manual.as_str(),
                match kind {
                    ModelValidationKind::Text => 1_i64,
                    ModelValidationKind::Vision => 2_i64,
                },
            ],
        )?;

        let entry = conn.query_row(
            "SELECT provider_id, model_id, display_name, source, stale,
                    first_seen_at, last_seen_at, last_refreshed_at,
                    text_verified_at, vision_verified_at, user_confirmed_capabilities
             FROM llm_model_registry
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model_id],
            map_entry,
        )?;
        Ok(entry)
    })?;
    Ok(entry)
}

/// Return whether a registry row supports the requested capability slot.
pub fn supports_model_for_slot(entry: &ModelRegistryEntry, slot: CapabilitySlot) -> bool {
    match slot {
        CapabilitySlot::Fast | CapabilitySlot::Writer => true,
        CapabilitySlot::Vision => entry.vision_verified_at.is_some(),
        CapabilitySlot::LongContext | CapabilitySlot::Reasoner => false,
        CapabilitySlot::AgentTools
        | CapabilitySlot::Embedding
        | CapabilitySlot::Reranker
        | CapabilitySlot::LocalPrivate => false,
    }
}

pub fn entries_from_builtin_and_routing(
    routing: &crate::llm::config::LlmRoutingConfig,
    mut registry: Vec<ModelRegistryEntry>,
) -> Vec<ModelRegistryEntry> {
    let mut seen: std::collections::HashSet<(String, String)> = registry
        .iter()
        .map(|entry| (entry.provider_id.clone(), entry.model_id.clone()))
        .collect();

    for model in crate::llm::model_catalog::catalog_for_settings() {
        let key = (model.provider_id.to_string(), model.id.to_string());
        if seen.insert(key.clone()) {
            registry.push(ModelRegistryEntry {
                provider_id: key.0,
                model_id: key.1,
                display_name: model.display_name.to_string(),
                source: ModelRegistrySource::BuiltIn,
                stale: false,
                first_seen_at: None,
                last_seen_at: None,
                last_refreshed_at: None,
                text_verified_at: Some("built_in".into()),
                vision_verified_at: model.supports_vision.then(|| "built_in".into()),
                user_confirmed_capabilities: Vec::new(),
            });
        }
    }

    for (provider_id, row) in &routing.providers {
        if let Some(models) = &row.enabled_models {
            for model_id in models {
                let trimmed = model_id.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let key = (provider_id.clone(), trimmed.to_string());
                if seen.insert(key.clone()) {
                    registry.push(ModelRegistryEntry {
                        provider_id: key.0,
                        model_id: key.1.clone(),
                        display_name: key.1,
                        source: ModelRegistrySource::Manual,
                        stale: false,
                        first_seen_at: None,
                        last_seen_at: None,
                        last_refreshed_at: None,
                        text_verified_at: None,
                        vision_verified_at: None,
                        user_confirmed_capabilities: Vec::new(),
                    });
                }
            }
        }
    }

    registry.sort_by(|a, b| {
        a.provider_id
            .cmp(&b.provider_id)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
    registry
}

fn registry_entry(
    db: &Database,
    provider_id: &str,
    model_id: &str,
) -> AppResult<Option<ModelRegistryEntry>> {
    db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT provider_id, model_id, display_name, source, stale,
                    first_seen_at, last_seen_at, last_refreshed_at,
                    text_verified_at, vision_verified_at, user_confirmed_capabilities
             FROM llm_model_registry
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model_id],
            map_entry,
        )
        .optional()
        .map_err(Into::into)
    })
}

#[allow(dead_code)]
fn confirmed_capabilities(
    db: &Database,
    provider_id: &str,
    model_id: &str,
) -> AppResult<Vec<CapabilitySlot>> {
    Ok(registry_entry(db, provider_id, model_id)?
        .map(|entry| entry.user_confirmed_capabilities)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_types::CapabilitySlot;
    use crate::storage::db::Database;

    fn migrated_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn provider_discovered_models_default_to_no_vision_until_validated() {
        let db = migrated_db();

        upsert_provider_discovered_models(&db, "openai-compatible", vec!["unknown-vl".to_string()])
            .unwrap();

        assert!(!supports_model_for_slot(
            &list_registry_entries(&db).unwrap()[0],
            CapabilitySlot::Vision,
        ));

        let entry = mark_model_validated(
            &db,
            "openai-compatible",
            "unknown-vl",
            ModelValidationKind::Vision,
        )
        .unwrap();

        assert!(supports_model_for_slot(&entry, CapabilitySlot::Vision));
        assert!(entry.vision_verified_at.is_some());
    }

    #[test]
    fn confirm_capability_does_not_mark_model_as_live_validated() {
        let db = migrated_db();
        upsert_provider_discovered_models(&db, "custom", vec!["model-a".to_string()]).unwrap();

        let entry = confirm_capability(&db, "custom", "model-a", CapabilitySlot::Vision).unwrap();

        assert!(entry
            .user_confirmed_capabilities
            .contains(&CapabilitySlot::Vision));
        assert!(entry.text_verified_at.is_none());
        assert!(entry.vision_verified_at.is_none());
    }

    #[test]
    fn clear_invalid_vision_validations_removes_non_vision_catalog_dirty_state() {
        let db = migrated_db();
        let dirty = mark_model_validated(
            &db,
            "deepseek",
            "deepseek-v4-flash",
            ModelValidationKind::Vision,
        )
        .unwrap();
        assert!(dirty.vision_verified_at.is_some());

        let cleared = clear_invalid_vision_validations(&db).unwrap();
        let entry = registry_entry(&db, "deepseek", "deepseek-v4-flash")
            .unwrap()
            .unwrap();

        assert_eq!(cleared, 1);
        assert!(entry.vision_verified_at.is_none());
        assert!(!supports_model_for_slot(&entry, CapabilitySlot::Vision));
    }

    #[test]
    fn confirm_capability_preserves_existing_slots_and_deduplicates() {
        let db = migrated_db();
        upsert_provider_discovered_models(&db, "custom", vec!["model-a".to_string()]).unwrap();

        confirm_capability(&db, "custom", "model-a", CapabilitySlot::Writer).unwrap();
        confirm_capability(&db, "custom", "model-a", CapabilitySlot::Vision).unwrap();
        confirm_capability(&db, "custom", "model-a", CapabilitySlot::Vision).unwrap();

        let entries = list_registry_entries(&db).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].user_confirmed_capabilities,
            vec![CapabilitySlot::Writer, CapabilitySlot::Vision]
        );
    }

    #[test]
    fn confirm_capability_creates_manual_entry_when_missing() {
        let db = migrated_db();

        let entry =
            confirm_capability(&db, "custom", "manual-vision", CapabilitySlot::Vision).unwrap();

        assert_eq!(entry.provider_id, "custom");
        assert_eq!(entry.model_id, "manual-vision");
        assert_eq!(entry.source, ModelRegistrySource::Manual);
        assert!(!supports_model_for_slot(&entry, CapabilitySlot::Vision));
    }

    #[test]
    fn upsert_list_and_stale_tracking_round_trip_through_sqlite() {
        let db = migrated_db();
        upsert_provider_discovered_models(
            &db,
            "custom",
            vec!["model-a".to_string(), "model-b".to_string()],
        )
        .unwrap();
        upsert_provider_discovered_models(&db, "other", vec!["other-model".to_string()]).unwrap();
        upsert_provider_discovered_models(
            &db,
            "custom",
            vec!["model-b".to_string(), "model-c".to_string()],
        )
        .unwrap();

        let entries: Vec<_> = list_registry_entries(&db)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.provider_id == "custom")
            .collect();
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| (entry.model_id.as_str(), entry.stale))
            .collect();
        assert_eq!(
            ids,
            vec![("model-a", true), ("model-b", false), ("model-c", false)]
        );

        let model_b = entries
            .iter()
            .find(|entry| entry.model_id == "model-b")
            .unwrap();
        assert_eq!(model_b.provider_id, "custom");
        assert_eq!(model_b.display_name, "model-b");
        assert_eq!(model_b.source, ModelRegistrySource::ProviderDiscovered);
        assert!(model_b.first_seen_at.is_some());
        assert!(model_b.last_seen_at.is_some());
        assert!(model_b.last_refreshed_at.is_some());

        let all_entries = list_registry_entries(&db).unwrap();
        assert_eq!(all_entries.len(), 4);
    }

    #[test]
    fn legacy_enabled_models_are_exposed_as_manual_entries() {
        let mut routing = crate::llm::config::deepseek_defaults();
        routing.providers.insert(
            "deepseek".into(),
            crate::llm::config::ProviderOverride {
                base_url: None,
                label: None,
                default_model: None,
                enabled_models: Some(vec!["custom-deepseek-model".into()]),
            },
        );

        let entries = entries_from_builtin_and_routing(&routing, vec![]);

        assert!(entries.iter().any(|entry| {
            entry.provider_id == "deepseek"
                && entry.model_id == "custom-deepseek-model"
                && entry.source == ModelRegistrySource::Manual
        }));
    }
}
