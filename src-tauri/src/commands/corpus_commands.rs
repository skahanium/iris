//! Corpus configuration IPC (`.iris/corpora.toml`).

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;
use crate::knowledge::corpora::{load_corpora, save_corpora, CorpusEntry};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusListItem {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    pub kind: String,
    pub scenes: Vec<String>,
}

#[tauri::command]
pub fn corpus_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<CorpusListItem>> {
    let vault = state.vault_path()?;
    let config = load_corpora(&vault)?;
    Ok(config
        .corpus
        .into_iter()
        .map(|c| CorpusListItem {
            id: c.id,
            name: c.name,
            path_prefix: c.path_prefix,
            kind: c.kind,
            scenes: c.scenes,
        })
        .collect())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusUpsertPayload {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    pub kind: String,
    pub scenes: Vec<String>,
}

/// Insert or replace a corpus entry in `.iris/corpora.toml`.
#[tauri::command]
pub fn corpus_upsert(
    state: State<'_, Arc<AppState>>,
    entry: CorpusUpsertPayload,
) -> AppResult<()> {
    let vault = state.vault_path()?;
    let mut config = load_corpora(&vault)?;
    let new_entry = CorpusEntry {
        id: entry.id,
        name: entry.name,
        path_prefix: entry.path_prefix,
        kind: entry.kind,
        scenes: entry.scenes,
    };
    if let Some(existing) = config.corpus.iter_mut().find(|c| c.id == new_entry.id) {
        *existing = new_entry;
    } else {
        config.corpus.push(new_entry);
    }
    save_corpora(&vault, &config)
}
