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
        let mut stmt = conn.prepare("SELECT id, path, title FROM files ORDER BY title")?;
        let nodes: Vec<GraphNode> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn graph_queries_return_nodes_and_edges() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute_batch(
                "INSERT INTO files (id, path, title, content_hash, created_at, updated_at)
                 VALUES (1, 'a.md', 'Note A', 'aa', '', ''),
                        (2, 'b.md', 'Note B', 'bb', '', ''),
                        (3, 'c.md', 'Note C', 'cc', '', '');
                 INSERT INTO links (source_id, target_id, context)
                 VALUES (1, 2, 'link from A to B'),
                        (2, 3, 'link from B to C');",
            )
            .unwrap();

            // Nodes
            let nodes: Vec<GraphNode> = conn
                .prepare("SELECT id, path, title FROM files ORDER BY title")
                .unwrap()
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .unwrap()
                .flatten()
                .map(|(id, path, title)| {
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

            assert_eq!(nodes.len(), 3);
            let a = nodes.iter().find(|n| n.id == 1).unwrap();
            let b = nodes.iter().find(|n| n.id == 2).unwrap();
            let c = nodes.iter().find(|n| n.id == 3).unwrap();
            assert_eq!(a.link_count, 1);
            assert_eq!(b.link_count, 2);
            assert_eq!(c.link_count, 1);

            // Edges
            let edges: Vec<GraphEdge> = conn
                .prepare("SELECT source_id, target_id FROM links")
                .unwrap()
                .query_map([], |row| {
                    Ok(GraphEdge {
                        source: row.get(0)?,
                        target: row.get(1)?,
                    })
                })
                .unwrap()
                .flatten()
                .collect();
            assert_eq!(edges.len(), 2);
            Ok(())
        })
        .unwrap();
    }
}
