//! Genre template extraction.
//!
//! Phase B: skeleton — stores manual templates and provides retrieval.
//! Phase C+ will add LLM-based structure extraction from model essays.

use rusqlite::Connection;
use serde_json::Value;

use crate::error::AppResult;
use crate::knowledge::{content_hash, make_template_key, EXTRACTOR_VERSION};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GenreTemplate {
    pub id: i64,
    pub template_key: String,
    pub genre: String,
    pub subtype: Option<String>,
    pub structure: Value,
    pub common_phrases: Option<Value>,
    pub style_features: Option<Value>,
    pub user_confirmed: bool,
    pub usage_count: i64,
}

/// Store a template (upsert by template_key).
pub fn upsert_template(
    conn: &Connection,
    genre: &str,
    subtype: Option<&str>,
    source_path: &str,
    structure: &Value,
    common_phrases: Option<&Value>,
    style_features: Option<&Value>,
    user_confirmed: bool,
) -> AppResult<i64> {
    let key = make_template_key(genre, source_path);
    let now = chrono::Utc::now().to_rfc3339();
    let confirmed = if user_confirmed { 1 } else { 0 };

    conn.execute(
        "INSERT INTO genre_templates
         (template_key, genre, subtype, structure, common_phrases, style_features,
          source_content_hash, extractor_version, user_confirmed, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
         ON CONFLICT(template_key) DO UPDATE SET
           structure = excluded.structure,
           common_phrases = excluded.common_phrases,
           style_features = excluded.style_features,
           user_confirmed = excluded.user_confirmed,
           updated_at = excluded.updated_at",
        rusqlite::params![
            key,
            genre,
            subtype,
            serde_json::to_string(structure).unwrap_or_default(),
            common_phrases.map(|v| serde_json::to_string(v).unwrap_or_default()),
            style_features.map(|v| serde_json::to_string(v).unwrap_or_default()),
            content_hash(&serde_json::to_string(structure).unwrap_or_default()),
            EXTRACTOR_VERSION,
            confirmed,
            now,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get all templates for a given genre.
pub fn get_templates_by_genre(conn: &Connection, genre: &str) -> AppResult<Vec<GenreTemplate>> {
    let mut stmt = conn.prepare(
        "SELECT id, template_key, genre, subtype, structure, common_phrases,
                style_features, user_confirmed, usage_count
         FROM genre_templates WHERE genre = ?1 ORDER BY usage_count DESC"
    )?;

    let rows = stmt.query_map([genre], |row| {
        let user_confirmed: i64 = row.get(7)?;
        Ok(GenreTemplate {
            id: row.get(0)?,
            template_key: row.get(1)?,
            genre: row.get(2)?,
            subtype: row.get(3)?,
            structure: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
            common_phrases: row.get::<_, Option<String>>(5)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            style_features: row.get::<_, Option<String>>(6)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            user_confirmed: user_confirmed != 0,
            usage_count: row.get(8)?,
        })
    })?;

    Ok(rows.flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn upsert_and_retrieve_template() {
        let db = Database::open_in_memory().unwrap();
        let structure = serde_json::json!({
            "sections": [{"name": "标题"}, {"name": "引言"}]
        });

        db.with_conn(|conn| {
            upsert_template(conn, "报告", None, "/notes/report.md", &structure, None, None, false)?;
            let templates = get_templates_by_genre(conn, "报告")?;
            assert_eq!(templates.len(), 1);
            assert_eq!(templates[0].genre, "报告");
            Ok(())
        }).unwrap();
    }
}
