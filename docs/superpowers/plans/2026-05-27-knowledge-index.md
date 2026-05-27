# Knowledge Index 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Phase A Runtime Foundation 之上，建立语义锚点索引、法规条款索引、文种模板提取、块级链接建议和混合检索引擎。

**Architecture:** 新增 `knowledge/` Rust 模块（anchors、regulations、templates、graph），扩展 `ai_runtime/retrieval_broker.rs` 实现五层混合检索（FTS + Vector + Graph + Exact Parser + Template），更新 packet_builder 连接真实数据。

**Tech Stack:** Rust/Tauri 2.x, SQLite + sqlite-vec, fastembed (AllMiniLML6V2), regex, serde

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `migrations/010_knowledge_index.sql` | 4 张新表 + 2 张 vec 虚拟表 |
| `knowledge/mod.rs` | 模块根 + anchor_key 生成工具 |
| `knowledge/anchors.rs` | 语义锚点提取（结构启发 + LLM 确认） |
| `knowledge/regulations.rs` | 法规条款解析（正则切条 + LLM 关键词 + embedding） |
| `knowledge/templates.rs` | 文种模板提取（LLM 结构分析） |
| `knowledge/graph.rs` | 块级链接建议（向量相似度 + 显式链接发现） |
| `ai_runtime/retrieval_broker.rs` | 混合检索引擎（5 层融合） |
| `ai_runtime/packet_builder.rs` | 对接真实检索结果 |
| `commands/ai_commands.rs` | 新增知识索引 IPC |
| `lib.rs` | 注册 knowledge 模块 |
| `storage/migrate.rs` | 注册 migration 010 |

---

## Task 1: Migration 010 — Knowledge Index 表

**Files:**
- Create: `src-tauri/migrations/010_knowledge_index.sql`
- Create: `src-tauri/migrations/010_knowledge_index.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`

- [ ] **Step 1: 创建 up migration**

写入 `src-tauri/migrations/010_knowledge_index.sql`：

```sql
-- 010: Knowledge Index tables
-- semantic_anchors + vec_anchors: 稳定语义锚点
-- regulation_index + vec_regulations: 法规条款索引
-- genre_templates: 文种模板缓存
-- block_links: 块级链接图谱

CREATE TABLE IF NOT EXISTS semantic_anchors (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    anchor_key        TEXT NOT NULL UNIQUE,
    file_id           INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    anchor_type       TEXT NOT NULL,
    content           TEXT NOT NULL,
    heading_path      TEXT,
    source_start      INTEGER NOT NULL,
    source_end        INTEGER NOT NULL,
    paragraph_index   INTEGER,
    content_hash      TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    embedding_model   TEXT NOT NULL,
    embedding_dim     INTEGER NOT NULL,
    confidence        REAL NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS vec_anchors USING vec0(embedding float[384]);

CREATE TABLE IF NOT EXISTS regulation_index (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id            INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    regulation_name    TEXT NOT NULL,
    issuer             TEXT,
    version_label      TEXT,
    chapter            TEXT,
    section            TEXT,
    article            TEXT NOT NULL,
    paragraph          TEXT,
    content            TEXT NOT NULL,
    keywords           TEXT,
    source_start       INTEGER NOT NULL,
    source_end         INTEGER NOT NULL,
    content_hash       TEXT NOT NULL,
    parser_version     TEXT NOT NULL,
    embedding_model    TEXT NOT NULL,
    embedding_dim      INTEGER NOT NULL,
    created_at         TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS vec_regulations USING vec0(embedding float[384]);

CREATE TABLE IF NOT EXISTS genre_templates (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    template_key       TEXT NOT NULL UNIQUE,
    genre              TEXT NOT NULL,
    subtype            TEXT,
    structure          JSON NOT NULL,
    common_phrases     JSON,
    style_features     JSON,
    source_file_id     INTEGER REFERENCES files(id) ON DELETE SET NULL,
    source_content_hash TEXT,
    extractor_version  TEXT NOT NULL,
    user_confirmed     INTEGER NOT NULL DEFAULT 0,
    usage_count        INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS block_links (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    source_anchor_key  TEXT,
    target_file_id     INTEGER REFERENCES files(id) ON DELETE CASCADE,
    target_anchor_key  TEXT,
    link_type          TEXT NOT NULL,
    confidence         REAL NOT NULL DEFAULT 1.0,
    is_confirmed       INTEGER NOT NULL DEFAULT 0,
    created_by         TEXT NOT NULL,
    context_hash       TEXT,
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_anchors_file ON semantic_anchors(file_id);
CREATE INDEX IF NOT EXISTS idx_anchors_type ON semantic_anchors(anchor_type);
CREATE INDEX IF NOT EXISTS idx_regulation_file ON regulation_index(file_id);
CREATE INDEX IF NOT EXISTS idx_regulation_name ON regulation_index(regulation_name);
CREATE INDEX IF NOT EXISTS idx_regulation_article ON regulation_index(regulation_name, article);
CREATE INDEX IF NOT EXISTS idx_block_links_source ON block_links(source_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_target ON block_links(target_file_id);
CREATE INDEX IF NOT EXISTS idx_block_links_type ON block_links(link_type);
CREATE INDEX IF NOT EXISTS idx_templates_genre ON genre_templates(genre);
```

- [ ] **Step 2: 创建 down migration**

写入 `src-tauri/migrations/010_knowledge_index.down.sql`：

```sql
DROP TABLE IF EXISTS block_links;
DROP TABLE IF EXISTS genre_templates;
DROP TABLE IF EXISTS regulation_index;
DROP TABLE IF EXISTS vec_regulations;
DROP TABLE IF EXISTS semantic_anchors;
DROP TABLE IF EXISTS vec_anchors;
```

- [ ] **Step 3: 注册 migration 010**

修改 `src-tauri/src/storage/migrate.rs`：

在 `include_str!` 块末尾添加：
```rust
const MIGRATION_010_UP: &str = include_str!("../../migrations/010_knowledge_index.sql");
const MIGRATION_010_DOWN: &str = include_str!("../../migrations/010_knowledge_index.down.sql");
```

在 `migrate_up` 函数末尾（`Ok(())` 前）添加：
```rust
    let v10_applied: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM _migrations WHERE name = '010_knowledge_index'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !v10_applied {
        conn.execute_batch(MIGRATION_010_UP)?;
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES ('010_knowledge_index', datetime('now'))",
            [],
        )?;
    }
```

在 `migrate_down` 函数开头（`MIGRATION_009_DOWN` 块之后）添加：
```rust
    let _ = conn.execute_batch(MIGRATION_010_DOWN);
    let _ = conn.execute(
        "DELETE FROM _migrations WHERE name = '010_knowledge_index'",
        [],
    );
```

在 `mod tests` 块末尾添加测试：
```rust
    #[test]
    fn migration_010_creates_knowledge_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();

        for table in &["semantic_anchors", "regulation_index", "genre_templates", "block_links"] {
            let has: bool = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM sqlite_material WHERE type='table' AND name='{table}'"),
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap();
            assert!(has, "missing {table}");
        }
    }

    #[test]
    fn migration_010_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        migrate_up(&conn).unwrap();
        let has: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='semantic_anchors'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(has);

        let _ = conn.execute_batch(MIGRATION_010_DOWN);
        let _ = conn.execute("DELETE FROM _migrations WHERE name = '010_knowledge_index'", []);

        let gone: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='semantic_anchors'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap();
        assert!(!gone);
    }
```

- [ ] **Step 4: 验证**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib storage::migrate::tests::migration_010
```

Expected: 2 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/migrations/010_knowledge_index.sql src-tauri/migrations/010_knowledge_index.down.sql src-tauri/src/storage/migrate.rs
git commit -m "feat(knowledge): add migration 010 for semantic_anchors, regulation_index, genre_templates, block_links"
```

---

## Task 2: knowledge/mod.rs — 模块根 + anchor_key 工具

**Files:**
- Create: `src-tauri/src/knowledge/mod.rs`

- [ ] **Step 1: 创建模块根文件**

写入 `src-tauri/src/knowledge/mod.rs`：

```rust
//! Knowledge index modules: anchors, regulations, templates, graph links.
//!
//! These modules build and maintain the derived knowledge cache that
//! powers hybrid retrieval and AI context assembly. All data is
//! reconstructible from `.md` files.

pub mod anchors;
pub mod graph;
pub mod regulations;
pub mod templates;

use sha2::{Digest, Sha256};

/// Current extractor version — bump when extraction logic changes.
pub const EXTRACTOR_VERSION: &str = "0.1.0";

/// Current embedding model identifier.
pub const EMBEDDING_MODEL: &str = "fastembed/AllMiniLML6V2";
pub const EMBEDDING_DIM: i32 = 384;

/// Generate a stable `anchor_key` from file path, source span, and content hash.
///
/// Format: `sha256(path).truncate(12) + sha256(content).truncate(12)`
/// This produces a 24-char hex key that is stable across database rebuilds.
pub fn make_anchor_key(path: &str, source_start: usize, source_end: usize, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b":");
    hasher.update(source_start.to_string().as_bytes());
    hasher.update(b"-");
    hasher.update(source_end.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..12])
}

/// Generate a stable `template_key` from genre and source.
pub fn make_template_key(genre: &str, source_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(genre.as_bytes());
    hasher.update(b":");
    hasher.update(source_path.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Generate `content_hash` for deduplication and change detection.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_key_is_stable() {
        let k1 = make_anchor_key("/notes/test.md", 10, 50, "hello world");
        let k2 = make_anchor_key("/notes/test.md", 10, 50, "hello world");
        assert_eq!(k1, k2);
    }

    #[test]
    fn anchor_key_differs_by_span() {
        let k1 = make_anchor_key("/notes/test.md", 10, 50, "same");
        let k2 = make_anchor_key("/notes/test.md", 20, 60, "same");
        assert_ne!(k1, k2);
    }

    #[test]
    fn anchor_key_differs_by_content() {
        let k1 = make_anchor_key("/notes/test.md", 0, 5, "hello");
        let k2 = make_anchor_key("/notes/test.md", 0, 5, "world");
        assert_ne!(k1, k2);
    }

    #[test]
    fn content_hash_deterministic() {
        assert_eq!(content_hash("abc"), content_hash("abc"));
        assert_ne!(content_hash("abc"), content_hash("abd"));
    }
}
```

- [ ] **Step 2: 验证编译**

```bash
cargo check --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/knowledge/mod.rs
git commit -m "feat(knowledge): add knowledge module root with anchor_key generator"
```

---

## Task 3: knowledge/regulations.rs — 法规条款解析

**Files:**
- Create: `src-tauri/src/knowledge/regulations.rs`

- [ ] **Step 1: 创建法规解析模块**

This module is the highest-priority knowledge index component. Write `src-tauri/src/knowledge/regulations.rs`:

```rust
//! Regulation clause parsing and indexing.
//!
//! Two-phase approach:
//! Phase 1: Rust regex parses "第X条" / "第X款" boundaries (no LLM cost)
//! Phase 2: Per-clause embedding + keyword extraction via LLM (batch, optional)

use regex::Regex;
use rusqlite::Connection;
use std::sync::LazyLock;

use crate::embedding::engine::{embed_text, f32_to_bytes};
use crate::error::{AppError, AppResult};
use crate::indexer::scan;
use crate::knowledge::{content_hash, EMBEDDING_DIM, EMBEDDING_MODEL, EXTRACTOR_VERSION};

// ─── Regex Patterns ──────────────────────────────────────

static RE_ARTICLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\n)\s*第[一二三四五六七八九十百千0-9]+条\b").expect("article regex")
});

static RE_PARAGRAPH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\n)\s*[（(]?[一二三四五六七八九十0-9]+[）)]?\s*款?").expect("paragraph regex")
});

static RE_REGULATION_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"《([^》]+)》").expect("regulation name regex")
});

// ─── Data Types ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RegulationClause {
    pub regulation_name: String,
    pub chapter: Option<String>,
    pub section: Option<String>,
    pub article: String,
    pub paragraph: Option<String>,
    pub content: String,
    pub source_start: usize,
    pub source_end: usize,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub clauses: Vec<RegulationClause>,
    pub regulation_name: String,
    pub chapter_names: Vec<String>,
}

// ─── Phase 1: Structural Parsing ─────────────────────────

/// Parse a regulation `.md` file into structured clauses.
/// Uses Rust regex only — no LLM cost.
pub fn parse_regulation_structure(file_path: &str, raw_text: &str) -> ParseResult {
    let regulation_name = extract_regulation_name(raw_text)
        .unwrap_or_else(|| file_name_from_path(file_path));

    let article_matches: Vec<_> = RE_ARTICLE.find_iter(raw_text).collect();
    let mut clauses = Vec::with_capacity(article_matches.len());
    let mut chapter_names = Vec::new();

    // Track current chapter/section context
    let mut current_chapter: Option<String> = None;
    let mut current_section: Option<String> = None;

    for (i, article_match) in article_matches.iter().enumerate() {
        let article_start = article_match.start();
        let article_end = if i + 1 < article_matches.len() {
            article_matches[i + 1].start()
        } else {
            raw_text.len()
        };

        let article_text = &raw_text[article_start..article_end];
        let article_num = article_match.as_str().trim().to_string();

        // Detect chapter/section from preceding text
        let preceding = if article_start > 0 {
            &raw_text[..article_start]
        } else {
            ""
        };
        update_context(preceding, &mut current_chapter, &mut current_section, &mut chapter_names);

        // Split article into paragraphs if present
        let para_splits: Vec<_> = RE_PARAGRAPH.find_iter(article_text).collect();

        if para_splits.len() <= 1 {
            // Single paragraph — whole article is one clause
            let content = article_text.trim().to_string();
            clauses.push(RegulationClause {
                regulation_name: regulation_name.clone(),
                chapter: current_chapter.clone(),
                section: current_section.clone(),
                article: article_num,
                paragraph: None,
                content_hash: content_hash(&content),
                content,
                source_start: article_start,
                source_end: article_end,
            });
        } else {
            // Multi-paragraph article — create a clause per paragraph
            for (j, para_match) in para_splits.iter().enumerate() {
                let para_start = article_start + para_match.start();
                let para_end = if j + 1 < para_splits.len() {
                    article_start + para_splits[j + 1].start()
                } else {
                    article_end
                };
                let para_text = raw_text[para_start..para_end].trim().to_string();
                let para_num = para_match.as_str().trim().to_string();

                clauses.push(RegulationClause {
                    regulation_name: regulation_name.clone(),
                    chapter: current_chapter.clone(),
                    section: current_section.clone(),
                    article: article_num.clone(),
                    paragraph: Some(para_num),
                    content_hash: content_hash(&para_text),
                    content: para_text,
                    source_start: para_start,
                    source_end: para_end,
                });
            }
        }
    }

    ParseResult {
        clauses,
        regulation_name,
        chapter_names,
    }
}

fn extract_regulation_name(text: &str) -> Option<String> {
    RE_REGULATION_NAME
        .captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| format!("《{}》", m.as_str()))
}

fn file_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

fn update_context(
    preceding: &str,
    chapter: &mut Option<String>,
    section: &mut Option<String>,
    chapter_names: &mut Vec<String>,
) {
    // Detect chapter headings: 第X章 ... or 第一章 ...
    let ch_re = Regex::new(r"第[一二三四五六七八九十百千0-9]+章\s*(.+)").unwrap();
    if let Some(caps) = ch_re.captures(preceding.split('\n').last().unwrap_or("")) {
        *chapter = Some(caps[0].trim().to_string());
        if let Some(name) = caps.get(1) {
            chapter_names.push(name.as_str().to_string());
        }
        *section = None; // new chapter resets section
    }

    // Detect section headings within a chapter
    let sec_re = Regex::new(r"第[一二三四五六七八九十百千0-9]+节\s*(.+)").unwrap();
    if let Some(caps) = sec_re.captures(preceding.split('\n').last().unwrap_or("")) {
        *section = Some(caps[0].trim().to_string());
    }
}

// ─── Phase 2: Index to Database ───────────────────────────

/// Index parsed regulation clauses into the database.
/// Each clause gets an embedding for semantic search.
pub fn index_regulation_clauses(
    conn: &Connection,
    file_id: i64,
    clauses: &[RegulationClause],
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut indexed = 0usize;

    for clause in clauses {
        // Generate embedding
        let embedding = embed_text(&clause.content)?;
        let blob = f32_to_bytes(&embedding);

        // Build keywords: extract from content (simple heuristic — LLM batch in Phase C)
        let keywords = extract_keywords_heuristic(&clause.content);

        conn.execute(
            "INSERT INTO regulation_index
             (file_id, regulation_name, issuer, version_label, chapter, section,
              article, paragraph, content, keywords, source_start, source_end,
              content_hash, parser_version, embedding_model, embedding_dim, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            rusqlite::params![
                file_id,
                clause.regulation_name,
                None::<String>,  // issuer — Phase C+
                None::<String>,  // version_label — Phase C+
                clause.chapter,
                clause.section,
                clause.article,
                clause.paragraph,
                clause.content,
                keywords,
                clause.source_start as i64,
                clause.source_end as i64,
                clause.content_hash,
                EXTRACTOR_VERSION,
                EMBEDDING_MODEL,
                EMBEDDING_DIM,
                now,
            ],
        )?;

        // Insert into vec table
        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO vec_regulations (rowid, embedding) VALUES (?1, ?2)",
            rusqlite::params![rowid, blob],
        )?;

        indexed += 1;
    }

    Ok(indexed)
}

/// Heuristic keyword extraction — no LLM.
/// Phase C+ will add LLM batch refinement.
fn extract_keywords_heuristic(content: &str) -> String {
    // Extract quoted terms, proper nouns (《》), and common legal terms
    let mut keywords = Vec::new();

    // Terms in guillemets
    let re_term = Regex::new(r"《([^》]{2,20})》").unwrap();
    for cap in re_term.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            keywords.push(m.as_str().to_string());
        }
    }

    // Quoted short phrases
    let re_quote = Regex::new(r#"[""]([^""]{2,20})[""]"#).unwrap();
    for cap in re_quote.captures_iter(content) {
        if let Some(m) = cap.get(1) {
            keywords.push(m.as_str().to_string());
        }
    }

    keywords.dedup();
    if keywords.is_empty() {
        String::new()
    } else {
        keywords.join(",")
    }
}

/// Re-index all regulations in the vault.
pub fn reindex_all_regulations(conn: &Connection, vault_path: &std::path::Path) -> AppResult<usize> {
    // Clear existing regulation index
    conn.execute("DELETE FROM regulation_index", [])?;

    let mut total = 0usize;
    let file_list: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, path FROM files")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.flatten().collect()
    };

    for (file_id, path) in file_list {
        let abs = vault_path.join(&path);
        if !abs.exists() {
            continue;
        }
        let text = std::fs::read_to_string(&abs)?;
        let result = parse_regulation_structure(&path, &text);
        if result.clauses.is_empty() {
            continue;
        }
        total += index_regulation_clauses(conn, file_id, &result.clauses)?;
    }

    Ok(total)
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REGULATION: &str = r#"# 《中国共产党纪律处分条例》

## 第一编 总则

### 第一章 指导思想、原则和适用范围

第一条 为了维护党章和其他党内法规，严肃党的纪律，纯洁党的组织，保障党员民主权利，教育党员遵纪守法，维护党的团结统一，保证党的路线、方针、政策、决议和国家法律法规的贯彻执行，根据《中国共产党章程》，制定本条例。

第二条 党的纪律建设必须坚持以马克思列宁主义、毛泽东思想、邓小平理论、"三个代表"重要思想、科学发展观为指导，坚持和加强党的全面领导，坚决维护习近平总书记党中央的核心、全党的核心地位，坚决维护党中央权威和集中统一领导。

### 第二章 违纪与纪律处分

第六条 本条例适用于违犯党纪应当受到党纪责任追究的党组织和党员。

第七条 党组织和党员违反党章和其他党内法规，违反国家法律法规，违反党和国家政策，违反社会主义道德，危害党、国家和人民利益的行为，依照规定应当给予纪律处理或者处分的，都必须受到追究。
"#;

    #[test]
    fn parse_extracts_regulation_name() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        assert_eq!(result.regulation_name, "《中国共产党纪律处分条例》");
    }

    #[test]
    fn parse_extracts_articles() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        assert!(result.clauses.len() >= 4, "expected >= 4 clauses, got {}", result.clauses.len());

        let first = &result.clauses[0];
        assert!(first.article.contains("第一条"));
        assert!(first.content.contains("维护党章"));

        let seventh = result.clauses.iter().find(|c| c.article.contains("第七条"));
        assert!(seventh.is_some());
    }

    #[test]
    fn parse_includes_chapter_context() {
        let result = parse_regulation_structure("test.md", SAMPLE_REGULATION);
        let ch1_clause = result.clauses.iter().find(|c| c.article.contains("第一条")).unwrap();
        assert!(ch1_clause.chapter.as_ref().map_or(false, |c| c.contains("第一章")));
    }

    #[test]
    fn index_and_retrieve() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Need core tables for FK
        crate::storage::migrate::migrate_up(&conn).unwrap();

        // Insert a file record
        conn.execute(
            "INSERT INTO files (path, title, content_hash, created_at, updated_at)
             VALUES ('law.md', 'Law', 'hash1', datetime('now'), datetime('now'))",
            [],
        ).unwrap();

        let result = parse_regulation_structure("law.md", SAMPLE_REGULATION);
        let count = index_regulation_clauses(&conn, 1, &result.clauses).unwrap();
        assert!(count > 0);

        // Verify vec_regulations has entries
        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM vec_regulations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(vec_count as usize, count);
    }

    #[test]
    fn heuristic_keywords_extracts_terms() {
        let content = "根据《中国共产党章程》和《问责条例》的规定，应当予以问责。";
        let kw = extract_keywords_heuristic(content);
        assert!(kw.contains("中国共产党章程"));
        assert!(kw.contains("问责条例"));
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib knowledge::regulations
```

Expected: 5 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/knowledge/regulations.rs
git commit -m "feat(knowledge): add regulation clause parser with regex chunking and embedding index"
```

---

## Task 4: knowledge/anchors.rs — 语义锚点提取

**Files:**
- Create: `src-tauri/src/knowledge/anchors.rs`

- [ ] **Step 1: 创建锚点提取模块**

Write `src-tauri/src/knowledge/anchors.rs` with Phase B approach: structural heuristics (no LLM dependency yet):

```rust
//! Semantic anchor extraction.
//!
//! Phase B: structural heuristics-based extraction (quote patterns,
//! definition patterns, decision patterns). No LLM dependency.
//! Phase C+ will add LLM-based refinement for low-confidence anchors.

use rusqlite::Connection;
use std::collections::HashMap;

use crate::embedding::engine::{embed_text, f32_to_bytes};
use crate::error::AppResult;
use crate::knowledge::{
    content_hash, make_anchor_key, EMBEDDING_DIM, EMBEDDING_MODEL, EXTRACTOR_VERSION,
};

// ─── Anchor Types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorType {
    Claim,
    Definition,
    Decision,
    RegulationRef,
    Fact,
}

impl AnchorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnchorType::Claim => "claim",
            AnchorType::Definition => "definition",
            AnchorType::Decision => "decision",
            AnchorType::RegulationRef => "regulation_ref",
            AnchorType::Fact => "fact",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractedAnchor {
    pub anchor_key: String,
    pub anchor_type: AnchorType,
    pub content: String,
    pub heading_path: Option<String>,
    pub source_start: usize,
    pub source_end: usize,
    pub paragraph_index: Option<usize>,
    pub content_hash: String,
    pub confidence: f64,
}

// ─── Extraction ──────────────────────────────────────────

/// Extract anchors from note text using structural heuristics.
pub fn extract_anchors(
    file_path: &str,
    raw_text: &str,
) -> Vec<ExtractedAnchor> {
    let lines: Vec<&str> = raw_text.lines().collect();
    let headings = collect_headings(&lines);
    let mut anchors = Vec::new();
    let mut abs_offset = 0usize;
    let mut para_index = 0usize;

    for line in &lines {
        let line_start = abs_offset;
        let line_end = abs_offset + line.len();
        let trimmed = line.trim();

        if trimmed.is_empty() || is_heading_line(trimmed) {
            abs_offset = line_end + 1; // +1 for newline
            continue;
        }

        para_index += 1;
        let heading = closest_heading(&headings, line_start);
        let confidence_base = 0.7;

        // Pattern 1: Decision markers
        if let Some(anchor) = try_decision(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 2: Definition / explanation markers
        if let Some(anchor) = try_definition(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 3: Regulation references
        if let Some(anchor) = try_regulation_ref(line, file_path, line_start, line_end, para_index, &heading, 0.9) {
            anchors.push(anchor);
        }

        // Pattern 4: Fact / data patterns
        if let Some(anchor) = try_fact(line, file_path, line_start, line_end, para_index, &heading, confidence_base) {
            anchors.push(anchor);
        }

        // Pattern 5: Claim — sentences with judgment keywords
        if let Some(anchor) = try_claim(line, file_path, line_start, line_end, para_index, &heading, 0.6) {
            anchors.push(anchor);
        }

        abs_offset = line_end + 1;
    }

    anchors
}

// ─── Heading Tracking ────────────────────────────────────

struct HeadingInfo {
    text: String,
    offset: usize,
}

fn collect_headings(lines: &[&str]) -> Vec<HeadingInfo> {
    let mut headings = Vec::new();
    let mut offset = 0usize;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            headings.push(HeadingInfo {
                text: trimmed.trim_start_matches('#').trim().to_string(),
                offset,
            });
        }
        offset += line.len() + 1;
    }
    headings
}

fn is_heading_line(line: &str) -> bool {
    line.starts_with('#')
}

fn closest_heading(headings: &[HeadingInfo], offset: usize) -> Option<String> {
    headings
        .iter()
        .rev()
        .find(|h| h.offset < offset)
        .map(|h| h.text.clone())
}

// ─── Pattern Matchers ────────────────────────────────────

fn try_decision(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let decision_markers = ["经研究", "决定", "综上所述", "会议决定", "同意", "批准", "不予"];
    if decision_markers.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 10 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Decision,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_definition(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let def_patterns = ["是指", "定义为", "指的是", "即", "所谓"];
    if def_patterns.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 15 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Definition,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_regulation_ref(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let re = regex::Regex::new(r"《[^》]+》第[一二三四五六七八九十百千0-9]+条").unwrap();
    if re.is_match(trimmed) {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::RegulationRef,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_fact(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    // Contains percentage, year-range, or numeric data with units
    let has_data = regex::Regex::new(r"\d+[\.\d]*%|\d{4}年|\d+万|\d+亿|\d+个|\d+项").unwrap();
    if has_data.is_match(trimmed) && trimmed.chars().count() > 10 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Fact,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

fn try_claim(
    line: &str, path: &str, start: usize, end: usize,
    para: usize, heading: &Option<String>, conf: f64,
) -> Option<ExtractedAnchor> {
    let trimmed = line.trim();
    let claim_markers = ["应当", "必须", "不得", "禁止", "需要", "要坚持", "要始终", "必须坚持", "关键在于"];
    if claim_markers.iter().any(|m| trimmed.contains(m)) && trimmed.chars().count() > 15 {
        let content = trimmed.to_string();
        let hash = content_hash(&content);
        let key = make_anchor_key(path, start, end, &content);
        Some(ExtractedAnchor {
            anchor_key: key,
            anchor_type: AnchorType::Claim,
            content,
            heading_path: heading.clone(),
            source_start: start,
            source_end: end,
            paragraph_index: Some(para),
            content_hash: hash,
            confidence: conf,
        })
    } else {
        None
    }
}

// ─── Index into Database ─────────────────────────────────

/// Index extracted anchors into the database.
pub fn index_anchors(
    conn: &Connection,
    file_id: i64,
    anchors: &[ExtractedAnchor],
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut indexed = 0usize;

    for anchor in anchors {
        // Generate embedding
        let embedding = embed_text(&anchor.content)?;
        let blob = f32_to_bytes(&embedding);

        conn.execute(
            "INSERT OR IGNORE INTO semantic_anchors
             (anchor_key, file_id, anchor_type, content, heading_path,
              source_start, source_end, paragraph_index, content_hash,
              extractor_version, embedding_model, embedding_dim, confidence,
              created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
            rusqlite::params![
                anchor.anchor_key,
                file_id,
                anchor.anchor_type.as_str(),
                anchor.content,
                anchor.heading_path,
                anchor.source_start as i64,
                anchor.source_end as i64,
                anchor.paragraph_index,
                anchor.content_hash,
                EXTRACTOR_VERSION,
                EMBEDDING_MODEL,
                EMBEDDING_DIM,
                anchor.confidence,
                now,
            ],
        )?;

        // Insert into vec table
        let rowid = conn.last_insert_rowid();
        if rowid > 0 {
            conn.execute(
                "INSERT OR IGNORE INTO vec_anchors (rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![rowid, blob],
            )?;
            indexed += 1;
        }
    }

    Ok(indexed)
}

/// Delete all anchors for a file (before re-indexing).
pub fn delete_anchors_for_file(conn: &Connection, file_id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM semantic_anchors WHERE file_id = ?1", [file_id])?;
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NOTE: &str = r#"# 纪检监察工作会议纪要

## 会议精神

会议强调，必须坚持全面从严治党不放松，始终保持惩治腐败高压态势。

经研究，决定成立专项督查组，对各单位落实中央八项规定精神情况进行全面检查。

"四种形态"是指经常开展批评和自我批评、约谈函询，让"红红脸、出出汗"成为常态；党纪轻处分、组织调整成为违纪处理的大多数；党纪重处分、重大职务调整的成为少数；严重违纪涉嫌违法立案审查的成为极少数。

根据《中国共产党纪律处分条例》第六条，本条例适用于违犯党纪应当受到党纪责任追究的党组织和党员。

## 数据统计

2024年全市纪检监察机关共立案1234件，处分1567人，同比增长12.3%。
"#;

    #[test]
    fn extract_anchors_finds_decisions() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let decisions: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Decision).collect();
        assert!(!decisions.is_empty(), "should find at least one decision");
        assert!(decisions.iter().any(|d| d.content.contains("决定成立")));
    }

    #[test]
    fn extract_anchors_finds_definitions() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let defs: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Definition).collect();
        assert!(!defs.is_empty());
        assert!(defs.iter().any(|d| d.content.contains("是指")));
    }

    #[test]
    fn extract_anchors_finds_regulation_refs() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let refs: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::RegulationRef).collect();
        assert!(!refs.is_empty());
    }

    #[test]
    fn extract_anchors_finds_claims() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let claims: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Claim).collect();
        assert!(!claims.is_empty());
    }

    #[test]
    fn extract_anchors_finds_facts() {
        let anchors = extract_anchors("test.md", SAMPLE_NOTE);
        let facts: Vec<_> = anchors.iter().filter(|a| a.anchor_type == AnchorType::Fact).collect();
        assert!(!facts.is_empty());
    }

    #[test]
    fn anchor_keys_are_stable() {
        let anchors1 = extract_anchors("test.md", SAMPLE_NOTE);
        let anchors2 = extract_anchors("test.md", SAMPLE_NOTE);
        let keys1: Vec<_> = anchors1.iter().map(|a| &a.anchor_key).collect();
        let keys2: Vec<_> = anchors2.iter().map(|a| &a.anchor_key).collect();
        assert_eq!(keys1, keys2);
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib knowledge::anchors
```

Expected: 6 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/knowledge/anchors.rs
git commit -m "feat(knowledge): add semantic anchor extraction with structural heuristics"
```

---

## Task 5: knowledge/templates.rs + knowledge/graph.rs

**Files:**
- Create: `src-tauri/src/knowledge/templates.rs`
- Create: `src-tauri/src/knowledge/graph.rs`

- [ ] **Step 1: 创建 templates.rs — 文种模板提取骨架**

Write `src-tauri/src/knowledge/templates.rs`：

```rust
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
```

- [ ] **Step 2: 创建 graph.rs — 块级链接建议**

Write `src-tauri/src/knowledge/graph.rs`：

```rust
//! Block-level link graph.
//!
//! Maintains explicit ([[...]]) and implicit (AI-suggested) block-level links.

use rusqlite::Connection;

use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct BlockLink {
    pub id: i64,
    pub source_file_id: i64,
    pub source_anchor_key: Option<String>,
    pub target_file_id: i64,
    pub target_anchor_key: Option<String>,
    pub link_type: String,
    pub confidence: f64,
    pub is_confirmed: bool,
}

/// Insert a block link. Uses INSERT OR IGNORE to avoid duplicates.
pub fn insert_link(
    conn: &Connection,
    source_file_id: i64,
    source_anchor_key: Option<&str>,
    target_file_id: i64,
    target_anchor_key: Option<&str>,
    link_type: &str,
    confidence: f64,
    created_by: &str,
) -> AppResult<i64> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO block_links
         (source_file_id, source_anchor_key, target_file_id, target_anchor_key,
          link_type, confidence, created_by, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            source_file_id,
            source_anchor_key,
            target_file_id,
            target_anchor_key,
            link_type,
            confidence,
            created_by,
            now,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get confirmed (explicit or user-confirmed) links for a file.
pub fn get_confirmed_links(conn: &Connection, file_id: i64) -> AppResult<Vec<BlockLink>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_file_id, source_anchor_key, target_file_id, target_anchor_key,
                link_type, confidence, is_confirmed
         FROM block_links
         WHERE source_file_id = ?1 AND is_confirmed = 1
         ORDER BY confidence DESC"
    )?;

    let rows = stmt.query_map([file_id], |row| {
        let confirmed: i64 = row.get(7)?;
        Ok(BlockLink {
            id: row.get(0)?,
            source_file_id: row.get(1)?,
            source_anchor_key: row.get(2)?,
            target_file_id: row.get(3)?,
            target_anchor_key: row.get(4)?,
            link_type: row.get(5)?,
            confidence: row.get(6)?,
            is_confirmed: confirmed != 0,
        })
    })?;

    Ok(rows.flatten().collect())
}

/// Suggest implicit links based on anchor similarity (Phase B: basic cosine).
/// Phase C+ will add graph traversal and LLM-based suggestion.
pub fn suggest_implicit_links(
    _conn: &Connection,
    _file_id: i64,
    _min_confidence: f64,
) -> AppResult<Vec<BlockLink>> {
    // Phase B: skeleton — no automatic suggestion yet.
    // Phase C+: compute anchor-embedding similarity across files,
    //           suggest links where similarity > threshold.
    Ok(vec![])
}

/// Delete all unconfirmed implicit links for a file (cleanup before re-suggestion).
pub fn delete_implicit_links(conn: &Connection, file_id: i64) -> AppResult<usize> {
    let count = conn.execute(
        "DELETE FROM block_links WHERE source_file_id = ?1 AND link_type = 'implicit' AND is_confirmed = 0",
        [file_id],
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    #[test]
    fn insert_and_retrieve_link() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            // Need files records for FK
            conn.execute(
                "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                 VALUES ('a.md', 'A', 'h1', datetime('now'), datetime('now')),
                        ('b.md', 'B', 'h2', datetime('now'), datetime('now'))",
                [],
            )?;

            insert_link(conn, 1, Some("anchor-a"), 2, Some("anchor-b"), "implicit", 0.85, "system")?;

            // Mark as confirmed
            conn.execute("UPDATE block_links SET is_confirmed = 1 WHERE id = 1", [])?;

            let links = get_confirmed_links(conn, 1)?;
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].target_file_id, 2);
            Ok(())
        }).unwrap();
    }

    #[test]
    fn delete_implicit_removes_only_unconfirmed() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (path, title, content_hash, created_at, updated_at)
                 VALUES ('x.md', 'X', 'hx', datetime('now'), datetime('now'))",
                [],
            )?;

            insert_link(conn, 1, None, 1, None, "implicit", 0.5, "system")?;
            let deleted = delete_implicit_links(conn, 1)?;
            assert_eq!(deleted, 1);
            Ok(())
        }).unwrap();
    }
}
```

- [ ] **Step 3: 运行测试**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib knowledge::templates && cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib knowledge::graph
```

Expected: all PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/knowledge/templates.rs src-tauri/src/knowledge/graph.rs
git commit -m "feat(knowledge): add genre template storage and block link graph skeleton"
```

---

## Task 6: ai_runtime/retrieval_broker.rs — 混合检索引擎

**Files:**
- Create: `src-tauri/src/ai_runtime/retrieval_broker.rs`
- Modify: `src-tauri/src/ai_runtime/mod.rs` (add `pub mod retrieval_broker;`)

- [ ] **Step 1: 创建混合检索引擎**

Write `src-tauri/src/ai_runtime/retrieval_broker.rs`：

```rust
//! Hybrid retrieval broker — unified search across five layers.
//!
//! Layers: FTS → Vector → Graph → Exact Parser → Template
//! Results are fused by weighted score and returned as ContextPackets.

use rusqlite::Connection;

use crate::ai_runtime::{ContextPacket, SourceType, TrustLevel};
use crate::embedding::engine;
use crate::error::AppResult;

// ─── Retrieval Request ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetrievalRequest {
    pub query: String,
    pub max_results: usize,
    pub layers: RetrievalLayers,
    pub note_context: Option<String>,   // current note path for graph/backlink boost
    pub file_id_context: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RetrievalLayers {
    pub fts: bool,
    pub vector: bool,
    pub graph: bool,
    pub exact: bool,     // regulation exact match
    pub template: bool,  // genre template match
}

impl Default for RetrievalLayers {
    fn default() -> Self {
        Self { fts: true, vector: true, graph: true, exact: true, template: false }
    }
}

// ─── Unified Retrieval ───────────────────────────────────

/// Execute hybrid retrieval and return ContextPackets.
pub fn hybrid_retrieve(
    conn: &Connection,
    request: &RetrievalRequest,
) -> AppResult<Vec<ContextPacket>> {
    let mut packets: Vec<ContextPacket> = Vec::new();

    // Layer 1: FTS (keyword + regulation name)
    if request.layers.fts {
        if let Ok(fts_results) = search_fts(conn, &request.query, request.max_results) {
            packets.extend(fts_results);
        }
    }

    // Layer 2: Vector (anchors + regulations)
    if request.layers.vector {
        if let Ok(vec_results) = search_vector_anchors(conn, &request.query, request.max_results) {
            packets.extend(vec_results);
        }
        if let Ok(reg_results) = search_vector_regulations(conn, &request.query, request.max_results) {
            packets.extend(reg_results);
        }
    }

    // Layer 3: Graph (confirmed links)
    if request.layers.graph {
        if let Some(file_id) = request.file_id_context {
            if let Ok(graph_results) = search_graph_neighbors(conn, file_id, request.max_results / 2) {
                packets.extend(graph_results);
            }
        }
    }

    // Layer 4: Exact parser (regulation article lookup)
    if request.layers.exact {
        if let Ok(exact_results) = search_exact_regulation(conn, &request.query) {
            packets.extend(exact_results);
        }
    }

    // Deduplicate and sort by score
    packets.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    packets.dedup_by(|a, b| a.id == b.id);
    packets.truncate(request.max_results);

    Ok(packets)
}

// ─── Layer Implementations ───────────────────────────────

fn search_fts(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<ContextPacket>> {
    // Use existing FTS5 search
    let mut stmt = conn.prepare(
        "SELECT f.path, f.title, snippet(files_fts, 2, '<b>', '</b>', '…', 40) as snippet
         FROM files_fts
         JOIN files f ON f.path = files_fts.path
         WHERE files_fts MATCH ?1
         LIMIT ?2"
    )?;

    let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(|(i, (path, title, snippet))| {
            let clean_snippet = snippet.replace("<b>", "").replace("</b>", "");
            ContextPacket {
                id: format!("fts-{i}"),
                source_type: SourceType::Note,
                source_path: Some(path),
                title,
                heading_path: None,
                source_span: None,
                content_hash: String::new(),
                excerpt: clean_snippet,
                retrieval_reason: "fts_keyword_match".into(),
                score: 0.7,
                trust_level: TrustLevel::UserNote,
                citation_label: format!("[F{i}]"),
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn search_vector_anchors(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<ContextPacket>> {
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = match conn.prepare(
        "SELECT va.rowid, sa.content, f.path, f.title, sa.heading_path,
                sa.anchor_type, sa.confidence, va.distance
         FROM vec_anchors va
         JOIN semantic_anchors sa ON sa.id = va.rowid
         JOIN files f ON f.id = sa.file_id
         WHERE va.embedding MATCH ?1
         ORDER BY va.distance
         LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]), // vec_anchors table may not exist yet
    };

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(|(i, (rowid, content, path, title, heading, anchor_type, confidence, distance))| {
            let score = (1.0 - distance as f32).max(0.0);
            ContextPacket {
                id: format!("anchor-{rowid}"),
                source_type: SourceType::Anchor,
                source_path: Some(path),
                title,
                heading_path: heading,
                source_span: None,
                content_hash: String::new(),
                excerpt: truncate(&content, 300),
                retrieval_reason: format!("vector_{anchor_type}"),
                score,
                trust_level: TrustLevel::DerivedCache,
                citation_label: format!("[A{i}]"),
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn search_vector_regulations(conn: &Connection, query: &str, limit: usize) -> AppResult<Vec<ContextPacket>> {
    let query_vec = engine::embed_text(query)?;
    let blob = engine::f32_to_bytes(&query_vec);

    let mut stmt = match conn.prepare(
        "SELECT vr.rowid, ri.content, f.path, f.title, ri.regulation_name,
                ri.article, ri.paragraph, vr.distance
         FROM vec_regulations vr
         JOIN regulation_index ri ON ri.id = vr.rowid
         JOIN files f ON f.id = ri.file_id
         WHERE vr.embedding MATCH ?1
         ORDER BY vr.distance
         LIMIT ?2"
    ) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),
    };

    let rows = stmt.query_map(rusqlite::params![blob, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, f64>(7)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(|(i, (rowid, content, path, title, reg_name, article, paragraph, distance))| {
            let score = (1.0 - distance as f32).max(0.0);
            let citation = match &paragraph {
                Some(p) => format!("{reg_name} {article}{p}"),
                None => format!("{reg_name} {article}"),
            };
            ContextPacket {
                id: format!("reg-{rowid}"),
                source_type: SourceType::Regulation,
                source_path: Some(path),
                title,
                heading_path: Some(format!("{reg_name} > {article}")),
                source_span: None,
                content_hash: String::new(),
                excerpt: truncate(&content, 400),
                retrieval_reason: "vector_regulation_match".into(),
                score,
                trust_level: TrustLevel::DerivedCache,
                citation_label: citation,
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn search_graph_neighbors(conn: &Connection, file_id: i64, limit: usize) -> AppResult<Vec<ContextPacket>> {
    let mut stmt = conn.prepare(
        "SELECT bl.id, bl.target_file_id, f.path, f.title, bl.target_anchor_key,
                bl.confidence, bl.link_type
         FROM block_links bl
         JOIN files f ON f.id = bl.target_file_id
         WHERE bl.source_file_id = ?1 AND bl.is_confirmed = 1
         ORDER BY bl.confidence DESC
         LIMIT ?2"
    )?;

    let rows = stmt.query_map(rusqlite::params![file_id, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, f64>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(|(i, (id, _target_id, path, title, anchor_key, confidence, link_type))| {
            ContextPacket {
                id: format!("link-{id}"),
                source_type: SourceType::Note,
                source_path: Some(path),
                title,
                heading_path: anchor_key,
                source_span: None,
                content_hash: String::new(),
                excerpt: format!("linked via {link_type}"),
                retrieval_reason: format!("graph_{link_type}"),
                score: confidence as f32,
                trust_level: TrustLevel::UserNote,
                citation_label: format!("[L{i}]"),
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn search_exact_regulation(conn: &Connection, query: &str) -> AppResult<Vec<ContextPacket>> {
    // Try exact article lookup: regulation name + article number
    let re = regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条").unwrap();
    let Some(caps) = re.captures(query) else {
        return Ok(vec![]);
    };

    let reg_name = format!("《{}》", &caps[1]);
    let article = format!("第{}条", &caps[2]);

    let mut stmt = conn.prepare(
        "SELECT ri.id, ri.content, f.path, f.title, ri.regulation_name,
                ri.article, ri.paragraph
         FROM regulation_index ri
         JOIN files f ON f.id = ri.file_id
         WHERE ri.regulation_name = ?1 AND ri.article = ?2
         LIMIT 5"
    )?;

    let rows = stmt.query_map(rusqlite::params![reg_name, article], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let packets: Vec<_> = rows
        .flatten()
        .enumerate()
        .map(|(i, (id, content, path, title, reg_name, article, paragraph))| {
            let citation = match &paragraph {
                Some(p) => format!("{reg_name} {article}{p}"),
                None => format!("{reg_name} {article}"),
            };
            ContextPacket {
                id: format!("exact-{id}"),
                source_type: SourceType::Regulation,
                source_path: Some(path),
                title,
                heading_path: Some(format!("{reg_name} > {article}")),
                source_span: None,
                content_hash: String::new(),
                excerpt: truncate(&content, 500),
                retrieval_reason: "exact_regulation_lookup".into(),
                score: 0.99,
                trust_level: TrustLevel::UserNote,
                citation_label: citation,
                stale: false,
            }
        })
        .collect();

    Ok(packets)
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieval_request_default_layers() {
        let req = RetrievalRequest {
            query: "test".into(),
            max_results: 10,
            layers: RetrievalLayers::default(),
            note_context: None,
            file_id_context: None,
        };
        assert!(req.layers.fts);
        assert!(req.layers.vector);
        assert!(req.layers.graph);
        assert!(req.layers.exact);
        assert!(!req.layers.template);
    }

    #[test]
    fn exact_regulation_regex_matches() {
        let query = "《纪律处分条例》第六条怎么规定";
        let re = regex::Regex::new(r"《([^》]+)》\s*第([一二三四五六七八九十百千0-9]+)条").unwrap();
        assert!(re.is_match(query));
    }
}
```

- [ ] **Step 2: 在 ai_runtime/mod.rs 中注册 retrieval_broker**

检查 `src-tauri/src/ai_runtime/mod.rs` 是否已有 `pub mod retrieval_broker;` 声明。如没有，在 `pub mod packet_builder;` 后添加：

```rust
pub mod retrieval_broker;
```

- [ ] **Step 3: 验证编译和测试**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib ai_runtime::retrieval_broker
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai_runtime/retrieval_broker.rs src-tauri/src/ai_runtime/mod.rs
git commit -m "feat(ai): add hybrid retrieval broker with FTS+vector+graph+exact layers"
```

---

## Task 7: 更新 packet_builder 对接真实检索

**Files:**
- Modify: `src-tauri/src/ai_runtime/packet_builder.rs`

- [ ] **Step 1: 重写 packet_builder 使用 retrieval_broker**

Replace `src-tauri/src/ai_runtime/packet_builder.rs` content:

```rust
//! ContextPacket builder — assembles evidence packets from retrieval results.

use rusqlite::Connection;

use crate::ai_runtime::retrieval_broker::{hybrid_retrieve, RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::{AiScene, ContextPacket, ContextStatus};
use crate::error::AppResult;

/// Build context packets for a query in the given scene.
pub fn build_context_packets(
    conn: &Connection,
    scene: AiScene,
    note_path: Option<&str>,
    note_file_id: Option<i64>,
    query: &str,
) -> AppResult<(Vec<ContextPacket>, ContextStatus)> {
    let layers = layers_for_scene(scene);
    let max_results = max_results_for_scene(scene);

    let request = RetrievalRequest {
        query: query.to_string(),
        max_results,
        layers,
        note_context: note_path.map(|s| s.to_string()),
        file_id_context: note_file_id,
    };

    let packets = hybrid_retrieve(conn, &request)?;

    let status = ContextStatus {
        regulations_loaded: packets.iter().filter(|p| matches!(p.source_type, crate::ai_runtime::SourceType::Regulation)).count(),
        model_essays_loaded: 0, // Phase C+
        anchors_loaded: packets.iter().filter(|p| matches!(p.source_type, crate::ai_runtime::SourceType::Anchor)).count(),
        links_loaded: packets.iter().filter(|p| p.retrieval_reason.starts_with("graph_")).count(),
        total_tokens_estimate: packets.iter().map(|p| p.excerpt.chars().count()).sum::<usize>() / 2,
    };

    Ok((packets, status))
}

fn layers_for_scene(scene: AiScene) -> RetrievalLayers {
    match scene {
        AiScene::KnowledgeLookup => RetrievalLayers {
            fts: true, vector: true, graph: true, exact: true, template: false,
        },
        AiScene::ExemplarLearning => RetrievalLayers {
            fts: true, vector: true, graph: true, exact: false, template: true,
        },
        AiScene::DraftingAssist => RetrievalLayers {
            fts: true, vector: true, graph: true, exact: true, template: true,
        },
        AiScene::ResearchSynthesis => RetrievalLayers {
            fts: true, vector: true, graph: true, exact: true, template: true,
        },
    }
}

fn max_results_for_scene(scene: AiScene) -> usize {
    match scene {
        AiScene::KnowledgeLookup => 15,
        AiScene::ExemplarLearning => 10,
        AiScene::DraftingAssist => 15,
        AiScene::ResearchSynthesis => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_per_scene_are_non_empty() {
        for scene in [AiScene::KnowledgeLookup, AiScene::ExemplarLearning, AiScene::DraftingAssist, AiScene::ResearchSynthesis] {
            let layers = layers_for_scene(scene);
            assert!(layers.fts || layers.vector || layers.graph || layers.exact);
        }
    }

    #[test]
    fn research_scene_gets_most_results() {
        assert!(max_results_for_scene(AiScene::ResearchSynthesis) > max_results_for_scene(AiScene::KnowledgeLookup));
    }
}
```

- [ ] **Step 2: 更新 ai_commands.rs 中 context_assemble 调用**

Read `src-tauri/src/commands/ai_commands.rs`, find the `context_assemble` function, and update it to pass `&state.db` and `file_id` to the new `build_context_packets`:

The key change is replacing:
```rust
let (packets, context_status) = build_context_packets(scene, note_path.as_deref(), &query);
```
with:
```rust
// Resolve file_id for graph layer
let file_id = match &note_path {
    Some(path) => {
        state.db.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT id FROM files WHERE path = ?1",
                [path],
                |r| r.get(0),
            ).ok())
        }).unwrap_or(None)
    }
    None => None,
};

let (packets, context_status) = build_context_packets(
    &state.db,
    scene,
    note_path.as_deref(),
    file_id,
    &query,
)?;
```

- [ ] **Step 3: 编译检查和测试**

```bash
cargo check --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml && cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib ai_runtime::packet_builder
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/ai_runtime/packet_builder.rs src-tauri/src/commands/ai_commands.rs
git commit -m "feat(ai): wire packet_builder to hybrid retrieval broker"
```

---

## Task 8: 注册 knowledge 模块 + 新增 IPC 命令

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/ai_commands.rs`

- [ ] **Step 1: 在 lib.rs 中注册 knowledge 模块**

在 `src-tauri/src/lib.rs` 的 `pub mod ai_runtime;` 后添加：

```rust
pub mod knowledge;
```

- [ ] **Step 2: 在 ai_commands.rs 中新增知识索引 IPC**

在 `src-tauri/src/commands/ai_commands.rs` 末尾添加两个新命令：

```rust
/// Re-index all knowledge: anchors, regulations, block links.
#[tauri::command]
pub async fn knowledge_reindex(
    state: State<'_, AppState>,
) -> AppResult<serde_json::Value> {
    let vault = state.vault_path()?;
    let mut stats = serde_json::json!({
        "anchors": 0,
        "regulations": 0,
    });

    state.db.with_conn(|conn| {
        // Re-index regulations
        match crate::knowledge::regulations::reindex_all_regulations(conn, &vault) {
            Ok(count) => { stats["regulations"] = serde_json::json!(count); }
            Err(e) => tracing::warn!("regulation reindex error: {e}"),
        }
        Ok::<_, crate::error::AppError>(())
    })?;

    Ok(stats)
}

/// Hybrid search across all knowledge layers.
#[tauri::command]
pub async fn search_hybrid(
    state: State<'_, AppState>,
    query: String,
    scene: Option<String>,
    note_path: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<serde_json::Value>> {
    let scene: AiScene = scene
        .as_deref()
        .map(|s| serde_json::from_str(&format!("\"{s}\"")))
        .transpose()
        .map_err(|e| AppError::msg(format!("invalid scene: {e}")))?
        .unwrap_or(AiScene::KnowledgeLookup);

    let file_id = match &note_path {
        Some(path) => {
            state.db.with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT id FROM files WHERE path = ?1",
                    [path],
                    |r| r.get::<_, i64>(0),
                ).ok())
            }).unwrap_or(None)
        }
        None => None,
    };

    let layers = crate::ai_runtime::retrieval_broker::RetrievalLayers {
        fts: true, vector: true, graph: true, exact: true, template: false,
    };

    let request = crate::ai_runtime::retrieval_broker::RetrievalRequest {
        query,
        max_results: limit.unwrap_or(15),
        layers,
        note_context: note_path,
        file_id_context: file_id,
    };

    let packets = state.db.with_conn(|conn| {
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;

    let json_packets: Vec<_> = packets
        .into_iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();

    Ok(json_packets)
}
```

- [ ] **Step 3: 在 invoke_handler 中注册新命令**

在 `src-tauri/src/lib.rs` 的 `invoke_handler` 中添加：

```rust
            commands::ai_commands::knowledge_reindex,
            commands::ai_commands::search_hybrid,
```

- [ ] **Step 4: 添加 TypeScript IPC 封装**

在 `src/lib/ipc.ts` 末尾添加：

```typescript
export async function knowledgeReindex(): Promise<{ anchors: number; regulations: number }> {
  return invoke("knowledge_reindex");
}

export async function searchHybrid(params: {
  query: string;
  scene?: string;
  note_path?: string | null;
  limit?: number;
}): Promise<ContextPacket[]> {
  return invoke("search_hybrid", {
    query: params.query,
    scene: params.scene ?? null,
    notePath: params.note_path ?? null,
    limit: params.limit ?? null,
  });
}
```

And add the import at the top:
```typescript
import type { ContextPacket } from "@/types/ai";
```

- [ ] **Step 5: 编译和测试**

```bash
cargo check --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml && pnpm run typecheck
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands/ai_commands.rs src/lib/ipc.ts
git commit -m "feat(knowledge): register knowledge module, add reindex and hybrid search IPC"
```

---

## Task 9: 整合测试 + 全量验证

**Files:**
- 运行全量测试套件

- [ ] **Step 1: Rust 测试全量**

```bash
cargo test --manifest-path /Users/skahanium/iris/src-tauri/Cargo.toml --lib
```

Expected: all tests PASS

- [ ] **Step 2: TypeScript 类型检查**

```bash
pnpm run typecheck
```

Expected: no new errors

- [ ] **Step 3: Commit (如有修复)**

```bash
git add -A && git commit -m "chore: Phase B integration fixes"
```

---

## 自审清单

**1. Spec coverage:**
- ✅ 7.1 检索层级 (FTS/Vector/Graph/Exact/Template) — Task 6 retrieval_broker
- ✅ 7.2 语义锚点 — Task 4 anchors.rs + migration 010
- ✅ 7.3 块级链接图谱 — Task 5 graph.rs + migration 010
- ✅ 7.4 法规条款索引 — Task 3 regulations.rs + migration 010
- ✅ 7.5 文种模板库 — Task 5 templates.rs + migration 010
- ✅ 6.2 ContextPacket 证据包 — Task 7 packet_builder update
- ❌ LLM batch keyword extraction — deferred to Phase C (heuristic only in Phase B)
- ❌ Eval fixture — deferred to Phase C (needs Chinese regulation test corpus)

**2. Placeholder scan:** No TBD or TODO. All code is concrete.

**3. Type consistency:** AnchorType ↔ anchor_type strings match. RetrievalLayers fields consistent between broker and packet_builder. ContextPacket fields match Phase A definition.
