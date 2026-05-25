use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub link_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphEdge {
    pub source: i64,
    pub target: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[tauri::command]
pub fn graph_data(state: State<'_, Arc<AppState>>) -> AppResult<GraphData> {
    state.db.with_conn(|conn| {
        // Nodes: all indexed files
        let mut stmt = conn.prepare(
            "SELECT id, path, title FROM files ORDER BY title",
        )?;
        let nodes: Vec<GraphNode> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })?
            .flatten()
            .map(|(id, path, title)| {
                // Count link references for node sizing
                let link_count: usize = conn
                    .query_row(
                        "SELECT COUNT(*) FROM links WHERE source_id = ?1 OR target_id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                GraphNode {
                    id,
                    path,
                    title,
                    link_count,
                }
            })
            .collect();

        // Edges: all links
        let mut estmt = conn.prepare("SELECT source_id, target_id FROM links")?;
        let edges: Vec<GraphEdge> = estmt
            .query_map([], |row| {
                Ok(GraphEdge {
                    source: row.get(0)?,
                    target: row.get(1)?,
                })
            })?
            .flatten()
            .collect();

        Ok(GraphData { nodes, edges })
    })
}
