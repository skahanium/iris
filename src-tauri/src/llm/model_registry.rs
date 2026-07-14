use rusqlite::params;
#[cfg(test)]
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

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

fn map_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelRegistryEntry> {
    let source_raw: String = row.get(3)?;
    let source = ModelRegistrySource::from_db(&source_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
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
                  last_refreshed_at)
                 VALUES (?1, ?2, ?2, ?3, 0, datetime('now'), datetime('now'), datetime('now'))
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
                     text_verified_at, vision_verified_at
              FROM llm_model_registry";
    db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(&format!("{sql} ORDER BY provider_id, model_id"))?;
        let rows = stmt.query_map([], map_entry)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

/// Warn about catalog models whose live vision probe contradicts the static
/// catalog declaration.  Probe results are authoritative — the function no
/// longer deletes data; it only logs warnings so operators can review
/// catalog accuracy.
pub fn clear_invalid_vision_validations(db: &Database) -> AppResult<usize> {
    db.with_conn(|conn| {
        let mut conflicts = 0;
        for model in crate::llm::model_catalog::catalog()
            .iter()
            .filter(|model| !model.supports_vision)
        {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM llm_model_registry
                 WHERE provider_id = ?1
                   AND model_id = ?2
                   AND vision_verified_at IS NOT NULL
                   AND vision_verified_at != 'built_in'",
                params![model.provider_id, model.id],
                |row| row.get(0),
            )?;
            if count > 0 {
                conflicts += 1;
                tracing::warn!(
                    provider_id = %model.provider_id,
                    model_id = %model.id,
                    "Catalog declares no vision support but a live vision probe succeeded; \
                     probe result is authoritative — consider updating the catalog entry"
                );
            }
        }
        Ok(conflicts as usize)
    })
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
              last_refreshed_at, text_verified_at, vision_verified_at)
             VALUES (?1, ?2, ?2, ?3, 0, datetime('now'), datetime('now'), datetime('now'),
                     CASE WHEN ?4 = 1 THEN datetime('now') ELSE NULL END,
                     CASE WHEN ?4 = 2 THEN datetime('now') ELSE NULL END)
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
                    text_verified_at, vision_verified_at
             FROM llm_model_registry
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model_id],
            map_entry,
        )?;
        Ok(entry)
    })?;
    Ok(entry)
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

#[cfg(test)]
fn registry_entry(
    db: &Database,
    provider_id: &str,
    model_id: &str,
) -> AppResult<Option<ModelRegistryEntry>> {
    db.with_read_conn(|conn| {
        conn.query_row(
            "SELECT provider_id, model_id, display_name, source, stale,
                    first_seen_at, last_seen_at, last_refreshed_at,
                    text_verified_at, vision_verified_at
             FROM llm_model_registry
             WHERE provider_id = ?1 AND model_id = ?2",
            params![provider_id, model_id],
            map_entry,
        )
        .optional()
        .map_err(Into::into)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn migrated_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn provider_discovered_models_default_to_no_vision_until_validated() {
        let db = migrated_db();

        upsert_provider_discovered_models(&db, "openai-compatible", vec!["unknown-vl".to_string()])
            .unwrap();

        assert!(list_registry_entries(&db).unwrap()[0]
            .vision_verified_at
            .is_none());

        let entry = mark_model_validated(
            &db,
            "openai-compatible",
            "unknown-vl",
            ModelValidationKind::Vision,
        )
        .unwrap();

        assert!(entry.vision_verified_at.is_some());
    }

    #[test]
    fn live_validation_records_explicit_model_facts() {
        let db = migrated_db();

        let text_entry =
            mark_model_validated(&db, "custom", "plain-text", ModelValidationKind::Text).unwrap();

        assert!(text_entry.text_verified_at.is_some());
        assert!(text_entry.vision_verified_at.is_none());

        let vision_entry =
            mark_model_validated(&db, "custom", "multi-modal", ModelValidationKind::Vision)
                .unwrap();
        assert!(vision_entry.vision_verified_at.is_some());
    }

    #[test]
    fn clear_invalid_vision_validations_preserves_live_probe_results() {
        let db = migrated_db();
        // Simulate a live vision probe that succeeded on a model whose catalog
        // entry says supports_vision=false.  The probe result is authoritative
        // and must not be deleted by the cleanup routine.
        let probed = mark_model_validated(
            &db,
            "deepseek",
            "deepseek-v4-flash",
            ModelValidationKind::Vision,
        )
        .unwrap();
        assert!(probed.vision_verified_at.is_some());

        let conflicts = clear_invalid_vision_validations(&db).unwrap();
        let entry = registry_entry(&db, "deepseek", "deepseek-v4-flash")
            .unwrap()
            .unwrap();

        // Live probe results are authoritative – must never be cleared.
        // The function returns the count of catalog-probe conflicts found
        // (1 in this case: catalog says no vision, but a live probe succeeded).
        assert_eq!(conflicts, 1);
        assert!(entry.vision_verified_at.is_some());
        assert!(entry.vision_verified_at.is_some());
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
                model_capabilities: std::collections::HashMap::new(),
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
