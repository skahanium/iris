use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::commands::file::FileListItem;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
pub struct TagGroup {
    pub name: String,
    pub files: Vec<FileListItem>,
}

/// List all tags with associated files from the index.
#[tauri::command]
pub fn tag_list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<TagGroup>> {
    state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT t.name, f.path, f.title, f.updated_at
             FROM tags t
             JOIN file_tags ft ON ft.tag_id = t.id
             JOIN files f ON f.id = ft.file_id
             ORDER BY t.name, f.title",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                FileListItem {
                    path: row.get(1)?,
                    title: row.get(2)?,
                    updated_at: row.get(3)?,
                },
            ))
        })?;

        let mut groups: Vec<TagGroup> = Vec::new();
        for row in rows.flatten() {
            let (name, file) = row;
            if let Some(last) = groups.last_mut() {
                if last.name == name {
                    last.files.push(file);
                    continue;
                }
            }
            groups.push(TagGroup {
                name,
                files: vec![file],
            });
        }
        Ok(groups)
    })
}
