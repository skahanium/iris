//! In-memory classified document search index.
//!
//! Scans `.classified/` Markdown files (excluding `.classified/.iris-ai`),
//! splits them into heading-aware chunks, and ranks results by term frequency,
//! heading match, current-document boost, path similarity, and recency.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;

use crate::crypto::classified_io;
use crate::crypto::vault_key::{VaultKey, VAULT_KEY};
use crate::error::AppResult;

/// A chunk of a classified Markdown document.
#[derive(Debug, Clone)]
pub struct ClassifiedChunk {
    pub document_path: String,
    pub heading: Option<String>,
    pub content: String,
    pub start_line: usize,
    /// File modification time as seconds since epoch (for recency scoring).
    pub modified_epoch: u64,
}

/// A single search hit from the classified index.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedSearchHit {
    pub document_path: String,
    pub heading: Option<String>,
    pub snippet: String,
    pub score: f64,
}

static CLASSIFIED_INDEX: Mutex<Vec<ClassifiedChunk>> = Mutex::new(Vec::new());

fn vault_key_read() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    VAULT_KEY
        .get()
        .ok_or_else(|| crate::error::AppError::msg("保险库未初始化"))?
        .read()
        .map_err(|e| crate::error::AppError::msg(format!("VAULT_KEY lock error: {e}")))
}

fn require_unlocked() -> AppResult<std::sync::RwLockReadGuard<'static, VaultKey>> {
    let vk = vault_key_read()?;
    if !vk.is_unlocked() {
        return Err(crate::error::AppError::msg("保险库未解锁"));
    }
    Ok(vk)
}

/// Split Markdown content into heading-aware chunks.
///
/// Each chunk starts at a heading (`#`, `##`, `###`, etc.) and includes all
/// content until the next heading of equal or higher level, or end of file.
/// Content before the first heading is included as a chunk with `heading: None`.
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 {
        return None;
    }
    trimmed
        .chars()
        .nth(level)
        .filter(|ch| ch.is_whitespace())
        .map(|_| level)
}

fn split_into_chunks(
    content: &str,
    document_path: &str,
    modified_epoch: u64,
) -> Vec<ClassifiedChunk> {
    let mut chunks = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_level: Option<usize> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut current_start_line: usize = 1;

    for (line_idx, line) in content.lines().enumerate() {
        let line_number = line_idx + 1;
        let trimmed = line.trim_start();

        if let Some(level) = heading_level(line) {
            if current_level.is_some_and(|active| level > active) {
                current_lines.push(line.to_string());
                continue;
            }

            // Flush previous chunk if it has content
            let body = current_lines.join("\n").trim().to_string();
            if !body.is_empty() {
                chunks.push(ClassifiedChunk {
                    document_path: document_path.to_string(),
                    heading: current_heading.clone(),
                    content: body,
                    start_line: current_start_line,
                    modified_epoch,
                });
            }
            current_heading = Some(trimmed.trim_start_matches('#').trim().to_string());
            current_level = Some(level);
            current_lines.clear();
            current_start_line = line_number;
        } else {
            current_lines.push(line.to_string());
        }
    }

    // Flush trailing chunk
    let body = current_lines.join("\n").trim().to_string();
    if !body.is_empty() {
        chunks.push(ClassifiedChunk {
            document_path: document_path.to_string(),
            heading: current_heading,
            content: body,
            start_line: current_start_line,
            modified_epoch,
        });
    }

    chunks
}

/// Recursively scan `.classified/` for `.md` files, excluding `.iris-ai`.
fn scan_classified_md_files(vault: &Path, dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if name == ".iris-ai" {
                continue;
            }
            scan_classified_md_files(vault, &path, out);
        } else if name.ends_with(".md") {
            let rel = path
                .strip_prefix(vault)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

/// Build the in-memory classified chunk index from `.classified/` files.
///
/// Returns the chunks and also stores them in the global `CLASSIFIED_INDEX`.
pub fn build_classified_index(vault: &Path) -> AppResult<Vec<ClassifiedChunk>> {
    let _vk = require_unlocked()?;

    let classified_dir = vault.join(".classified");
    if !classified_dir.is_dir() {
        return Ok(Vec::new());
    }

    let key = *vault_key_read()?.key()?;

    let mut file_pairs = Vec::new();
    scan_classified_md_files(vault, &classified_dir, &mut file_pairs);

    let mut all_chunks = Vec::new();

    for (rel_path, abs_path) in file_pairs {
        let modified_epoch = fs::metadata(&abs_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let raw = match fs::read(&abs_path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let content = if classified_io::has_csef_magic(&raw) {
            match classified_io::decrypt_cef(&raw, &key) {
                Ok(decrypted) => String::from_utf8_lossy(&decrypted).into_owned(),
                Err(_) => continue,
            }
        } else {
            String::from_utf8_lossy(&raw).into_owned()
        };

        let chunks = split_into_chunks(&content, &rel_path, modified_epoch);
        all_chunks.extend(chunks);
    }

    if let Ok(mut index) = CLASSIFIED_INDEX.lock() {
        *index = all_chunks.clone();
    }

    Ok(all_chunks)
}

/// Search the given chunks (or the global index if empty) and return ranked hits.
pub fn search_chunks(
    chunks: &[ClassifiedChunk],
    query: &str,
    current_doc: Option<&str>,
    limit: usize,
) -> Vec<ClassifiedSearchHit> {
    if query.trim().is_empty() || chunks.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
    if query_terms.is_empty() {
        return Vec::new();
    }

    let now_epoch = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut scored: Vec<(f64, &ClassifiedChunk)> = chunks
        .iter()
        .filter_map(|chunk| {
            let content_lower = chunk.content.to_lowercase();
            let heading_lower = chunk
                .heading
                .as_deref()
                .map(|h| h.to_lowercase())
                .unwrap_or_default();

            // Term frequency score
            let mut tf_score: f64 = 0.0;
            for term in &query_terms {
                let content_count = content_lower.matches(term).count() as f64;
                tf_score += content_count;
                // Heading match bonus
                if heading_lower.contains(term) {
                    tf_score += 3.0;
                }
            }

            if tf_score <= 0.0 {
                return None;
            }

            // Normalize by content length to avoid biasing toward long chunks
            let normalized_tf = tf_score / (chunk.content.len() as f64).sqrt().max(1.0);

            // Current document boost
            let doc_boost = if let Some(current) = current_doc {
                if chunk.document_path == current {
                    1.5
                } else {
                    // Path similarity: shared prefix segments get a small boost
                    let shared = chunk
                        .document_path
                        .split('/')
                        .zip(current.split('/'))
                        .take_while(|(a, b)| a == b)
                        .count();
                    1.0 + (shared as f64 * 0.1).min(0.5)
                }
            } else {
                1.0
            };

            // Recency boost: newer files get up to 1.3x
            let age_secs = now_epoch.saturating_sub(chunk.modified_epoch);
            let age_days = age_secs as f64 / 86400.0;
            let recency_boost = 1.0 + (0.3 * (-age_days / 365.0).exp());

            let score = normalized_tf * doc_boost * recency_boost;

            Some((score, chunk))
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(limit)
        .map(|(score, chunk)| {
            let snippet = if chunk.content.len() > 300 {
                format!("{}…", &chunk.content[..300])
            } else {
                chunk.content.clone()
            };
            ClassifiedSearchHit {
                document_path: chunk.document_path.clone(),
                heading: chunk.heading.clone(),
                snippet,
                score,
            }
        })
        .collect()
}

/// Clear the in-memory classified retrieval index.
pub fn clear_classified_index() {
    if let Ok(mut index) = CLASSIFIED_INDEX.lock() {
        index.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_into_chunks_basic() {
        let content = "Preamble text.\n\n# Heading One\nBody one.\n\n## Sub Heading\nBody two.\n\n# Heading Two\nBody three.";
        let chunks = split_into_chunks(content, "test.md", 1000);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].heading.is_none());
        assert_eq!(chunks[0].content, "Preamble text.");
        assert_eq!(chunks[1].heading.as_deref(), Some("Heading One"));
        assert!(chunks[1].content.contains("Body one."));
        assert!(chunks[1].content.contains("## Sub Heading"));
        assert!(chunks[1].content.contains("Body two."));
        assert_eq!(chunks[2].heading.as_deref(), Some("Heading Two"));
        assert_eq!(chunks[2].content, "Body three.");
    }

    #[test]
    fn split_into_chunks_no_headings() {
        let content = "Just plain text.\nNo headings here.";
        let chunks = split_into_chunks(content, "plain.md", 2000);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].heading.is_none());
        assert_eq!(chunks[0].content, "Just plain text.\nNo headings here.");
    }

    #[test]
    fn split_into_chunks_empty_content() {
        let chunks = split_into_chunks("", "empty.md", 3000);
        assert!(chunks.is_empty());
    }

    #[test]
    fn search_chunks_term_frequency() {
        let chunks = vec![
            ClassifiedChunk {
                document_path: "a.md".into(),
                heading: Some("Alpha".into()),
                content: "rust is a systems programming language".into(),
                start_line: 1,
                modified_epoch: 1_700_000_000,
            },
            ClassifiedChunk {
                document_path: "b.md".into(),
                heading: Some("Beta".into()),
                content: "python is a scripting language".into(),
                start_line: 1,
                modified_epoch: 1_700_000_000,
            },
        ];
        let hits = search_chunks(&chunks, "rust systems", None, 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].document_path, "a.md");
    }

    #[test]
    fn search_chunks_current_doc_boost() {
        let chunks = vec![
            ClassifiedChunk {
                document_path: "notes/a.md".into(),
                heading: None,
                content: "alpha bravo charlie".into(),
                start_line: 1,
                modified_epoch: 1_700_000_000,
            },
            ClassifiedChunk {
                document_path: "notes/b.md".into(),
                heading: None,
                content: "alpha bravo charlie".into(),
                start_line: 1,
                modified_epoch: 1_700_000_000,
            },
        ];
        let hits = search_chunks(&chunks, "alpha", Some("notes/a.md"), 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].document_path, "notes/a.md");
    }

    #[test]
    fn search_chunks_empty_query_returns_empty() {
        let chunks = vec![ClassifiedChunk {
            document_path: "a.md".into(),
            heading: None,
            content: "some content".into(),
            start_line: 1,
            modified_epoch: 1_700_000_000,
        }];
        let hits = search_chunks(&chunks, "", None, 10);
        assert!(hits.is_empty());
    }

    #[test]
    fn search_chunks_limit_respected() {
        let chunks: Vec<ClassifiedChunk> = (0..20)
            .map(|i| ClassifiedChunk {
                document_path: format!("doc{i}.md"),
                heading: None,
                content: "matching keyword here".into(),
                start_line: 1,
                modified_epoch: 1_700_000_000,
            })
            .collect();
        let hits = search_chunks(&chunks, "matching", None, 5);
        assert_eq!(hits.len(), 5);
    }
}
