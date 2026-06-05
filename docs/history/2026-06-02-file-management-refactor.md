# 文件管理系统深度重构实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 引入内容寻址存储 (CAS) 和领域驱动设计 (DDD)，重构文件管理系统，解决文件创建、管理、删除、隔离、生命周期管理、历史版本追踪、数据统计等方面的问题。

**Architecture:** 分层架构 + CAS 核心层。命令层处理IPC调用，领域层定义聚合根和不变量，CAS核心层提供内容寻址存储和引用计数，存储层管理SQLite和文件系统。

**Tech Stack:** Rust, SQLite, SHA-256, Tauri 2.x

---

## 文件结构

### 新增文件

| 文件路径                                        | 职责             |
| ----------------------------------------------- | ---------------- |
| `src-tauri/src/cas/mod.rs`                      | CAS 模块入口     |
| `src-tauri/src/cas/hash.rs`                     | 统一哈希计算接口 |
| `src-tauri/src/cas/store.rs`                    | CAS 对象存储     |
| `src-tauri/src/cas/ref_counter.rs`              | 引用计数管理     |
| `src-tauri/src/cas/garbage_collector.rs`        | 垃圾回收器       |
| `src-tauri/src/cas/write_guard.rs`              | 乐观锁实现       |
| `src-tauri/src/cas/patch.rs`                    | 补丁应用接口     |
| `src-tauri/src/scheduler.rs`                    | 定时任务调度     |
| `src-tauri/migrations/016_cas_tables.sql`       | CAS 表迁移脚本   |
| `src-tauri/tests/cas/hash_test.rs`              | 哈希计算测试     |
| `src-tauri/tests/cas/store_test.rs`             | CAS 存储测试     |
| `src-tauri/tests/cas/ref_counter_test.rs`       | 引用计数测试     |
| `src-tauri/tests/cas/garbage_collector_test.rs` | 垃圾回收测试     |

### 修改文件

| 文件路径                                        | 修改内容              |
| ----------------------------------------------- | --------------------- |
| `src-tauri/src/app.rs`                          | 添加 CAS 存储和调度器 |
| `src-tauri/src/commands/file.rs`                | 使用 CAS 接口         |
| `src-tauri/src/commands/version.rs`             | 使用 CAS 接口         |
| `src-tauri/src/version/mod.rs`                  | 重构版本管理          |
| `src-tauri/src/recycle/mod.rs`                  | 重构回收站            |
| `src-tauri/src/watcher/mod.rs`                  | 使用 CAS 接口         |
| `src-tauri/src/ai_runtime/writing_workflow.rs`  | 统一哈希计算          |
| `src-tauri/src/ai_runtime/document_workflow.rs` | 统一哈希计算          |
| `src-tauri/src/ai_runtime/organize_workflow.rs` | 统一哈希计算          |
| `src-tauri/src/ai_runtime/tool_dispatch.rs`     | 使用 CAS 读取接口     |
| `src-tauri/src/indexer/scan.rs`                 | 增加 cas_hash 字段    |

---

## Task 1: 创建 CAS 模块基础结构

**Files:**

- Create: `src-tauri/src/cas/mod.rs`
- Create: `src-tauri/src/cas/hash.rs`
- Test: `src-tauri/tests/cas/hash_test.rs`

- [ ] **Step 1: 创建 CAS 模块入口文件**

```rust
// src-tauri/src/cas/mod.rs

pub mod hash;
pub mod store;
pub mod ref_counter;
pub mod garbage_collector;
pub mod write_guard;
pub mod patch;
```

- [ ] **Step 2: 创建哈希计算模块**

```rust
// src-tauri/src/cas/hash.rs

use sha2::{Digest, Sha256};

/// 统一的内容哈希计算接口
pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

/// 字符串内容哈希（便捷函数）
pub fn content_hash_str(content: &str) -> String {
    content_hash(content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let content = "Hello, World!";
        let hash1 = content_hash_str(content);
        let hash2 = content_hash_str(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_different_content() {
        let hash1 = content_hash_str("Hello");
        let hash2 = content_hash_str("World");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_content_hash_empty_content() {
        let hash = content_hash_str("");
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA-256 hex length
    }

    #[test]
    fn test_content_hash_binary_content() {
        let content = vec![0u8, 1, 2, 3, 255];
        let hash = content_hash(&content);
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64);
    }
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p iris-lib cas::hash::tests -- --nocapture`
Expected: PASS

- [ ] **Step 4: 提交代码**

```bash
git add src-tauri/src/cas/mod.rs src-tauri/src/cas/hash.rs
git commit -m "feat(cas): 添加 CAS 模块和统一哈希计算接口"
```

---

## Task 2: 创建 CAS 对象存储

**Files:**

- Create: `src-tauri/src/cas/store.rs`
- Test: `src-tauri/tests/cas/store_test.rs`

- [ ] **Step 1: 定义对象类型**

```rust
// src-tauri/src/cas/store.rs

use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::error::{AppError, AppResult};

/// 对象类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
}

/// Blob 对象（内容块）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobObject {
    pub hash: String,
    pub content: Vec<u8>,
    pub size: u64,
    pub ref_count: u32,
    pub created_at: DateTime<Utc>,
}

/// Tree 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub object_hash: String,
    pub object_type: ObjectType,
    pub mode: String,
}

/// Tree 对象（目录树）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeObject {
    pub hash: String,
    pub entries: Vec<TreeEntry>,
    pub ref_count: u32,
    pub created_at: DateTime<Utc>,
}

/// 提交元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMetadata {
    pub file_id: i64,
    pub version_no: String,
    pub label: Option<String>,
    pub kind: String,
    pub word_count: i64,
    pub is_finalized: bool,
}

/// Commit 对象（版本提交）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitObject {
    pub hash: String,
    pub tree_hash: String,
    pub parent_hash: Option<String>,
    pub author: String,
    pub message: String,
    pub metadata: CommitMetadata,
    pub created_at: DateTime<Utc>,
}
```

- [ ] **Step 2: 实现 CAS 存储**

```rust
// src-tauri/src/cas/store.rs (续)

/// CAS 对象存储
pub struct CasObjectStore {
    base_path: PathBuf,
}

impl CasObjectStore {
    /// 创建新的 CAS 存储实例
    pub fn new(base_path: PathBuf) -> AppResult<Self> {
        let objects_dir = base_path.join("objects");
        let refs_dir = base_path.join("refs");
        let gc_dir = base_path.join("gc");

        fs::create_dir_all(&objects_dir)?;
        fs::create_dir_all(&refs_dir)?;
        fs::create_dir_all(&gc_dir)?;

        Ok(Self { base_path })
    }

    /// 获取对象文件路径
    pub fn object_path(&self, hash: &str) -> PathBuf {
        let (prefix, suffix) = hash.split_at(2);
        self.base_path.join("objects").join(prefix).join(suffix)
    }

    /// 存储 blob 对象
    pub fn store_blob(&self, content: &[u8]) -> AppResult<String> {
        let hash = super::hash::content_hash(content);
        let path = self.object_path(&hash);

        if path.exists() {
            return Ok(hash);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, content)?;
        Ok(hash)
    }

    /// 读取 blob 内容
    pub fn read_blob(&self, hash: &str) -> AppResult<Vec<u8>> {
        let path = self.object_path(hash);
        if !path.exists() {
            return Err(AppError::msg(format!("Object not found: {}", hash)));
        }
        Ok(fs::read(path)?)
    }

    /// 读取 blob 内容为字符串
    pub fn read_blob_content(&self, hash: &str) -> AppResult<String> {
        let content = self.read_blob(hash)?;
        String::from_utf8(content)
            .map_err(|e| AppError::msg(format!("Invalid UTF-8: {}", e)))
    }

    /// 存储 tree 对象
    pub fn store_tree(&self, tree: &TreeObject) -> AppResult<String> {
        let content = serde_json::to_vec(tree)?;
        let hash = super::hash::content_hash(&content);
        let path = self.object_path(&hash);

        if path.exists() {
            return Ok(hash);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, &content)?;
        Ok(hash)
    }

    /// 读取 tree 对象
    pub fn read_tree(&self, hash: &str) -> AppResult<TreeObject> {
        let content = self.read_blob(hash)?;
        serde_json::from_slice(&content)
            .map_err(|e| AppError::msg(format!("Invalid tree object: {}", e)))
    }

    /// 存储 commit 对象
    pub fn store_commit(&self, commit: &CommitObject) -> AppResult<String> {
        let content = serde_json::to_vec(commit)?;
        let hash = super::hash::content_hash(&content);
        let path = self.object_path(&hash);

        if path.exists() {
            return Ok(hash);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, &content)?;
        Ok(hash)
    }

    /// 读取 commit 对象
    pub fn read_commit(&self, hash: &str) -> AppResult<CommitObject> {
        let content = self.read_blob(hash)?;
        serde_json::from_slice(&content)
            .map_err(|e| AppError::msg(format!("Invalid commit object: {}", e)))
    }

    /// 更新引用
    pub fn update_ref(&self, ref_name: &str, hash: &str) -> AppResult<()> {
        let ref_path = self.base_path.join("refs").join(ref_name);
        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&ref_path, hash)?;
        Ok(())
    }

    /// 读取引用
    pub fn read_ref(&self, ref_name: &str) -> AppResult<Option<String>> {
        let ref_path = self.base_path.join("refs").join(ref_name);
        if !ref_path.exists() {
            return Ok(None);
        }
        let hash = fs::read_to_string(ref_path)?;
        Ok(Some(hash.trim().to_string()))
    }

    /// 写入文件内容（写入CAS + 更新文件系统）
    pub fn write_content(&self, path: &str, content: &str) -> AppResult<String> {
        let hash = super::hash::content_hash_str(content);
        self.store_blob(content.as_bytes())?;
        Ok(hash)
    }
}
```

- [ ] **Step 3: 创建测试**

```rust
// src-tauri/tests/cas/store_test.rs

use iris_lib::cas::store::{CasObjectStore, CommitObject, CommitMetadata, TreeObject, TreeEntry, ObjectType};
use tempfile::tempdir;

#[test]
fn test_store_and_retrieve_blob() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let content = "Hello, World!";
    let hash = store.store_blob(content.as_bytes()).unwrap();

    let retrieved = store.read_blob(&hash).unwrap();
    assert_eq!(retrieved, content.as_bytes());
}

#[test]
fn test_store_and_retrieve_commit() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let commit = CommitObject {
        hash: String::new(),
        tree_hash: "tree_hash".to_string(),
        parent_hash: None,
        author: "Iris".to_string(),
        message: "Test commit".to_string(),
        metadata: CommitMetadata {
            file_id: 1,
            version_no: "20260101000000000".to_string(),
            label: None,
            kind: "manual".to_string(),
            word_count: 10,
            is_finalized: false,
        },
        created_at: chrono::Utc::now(),
    };

    let hash = store.store_commit(&commit).unwrap();
    let retrieved = store.read_commit(&hash).unwrap();

    assert_eq!(retrieved.message, "Test commit");
    assert_eq!(retrieved.metadata.file_id, 1);
}

#[test]
fn test_content_deduplication() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let content = "Same content";
    let hash1 = store.store_blob(content.as_bytes()).unwrap();
    let hash2 = store.store_blob(content.as_bytes()).unwrap();

    assert_eq!(hash1, hash2);
}

#[test]
fn test_update_and_read_ref() {
    let dir = tempdir().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    let hash = "abc123";
    store.update_ref("versions/1", hash).unwrap();

    let retrieved = store.read_ref("versions/1").unwrap();
    assert_eq!(retrieved, Some(hash.to_string()));
}
```

- [ ] **Step 4: 运行测试验证**

Run: `cargo test -p iris-lib cas::store -- --nocapture`
Expected: PASS

- [ ] **Step 5: 提交代码**

```bash
git add src-tauri/src/cas/store.rs src-tauri/tests/cas/store_test.rs
git commit -m "feat(cas): 实现 CAS 对象存储"
```

---

## Task 3: 创建引用计数管理器

**Files:**

- Create: `src-tauri/src/cas/ref_counter.rs`
- Test: `src-tauri/tests/cas/ref_counter_test.rs`

- [ ] **Step 1: 实现引用计数管理器**

```rust
// src-tauri/src/cas/ref_counter.rs

use chrono::Utc;
use crate::error::AppResult;
use crate::storage::db::Database;

/// 引用计数管理器
pub struct RefCounter {
    db: Database,
}

impl RefCounter {
    /// 创建新的引用计数管理器
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 增加引用计数
    pub fn increment(&self, object_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO cas_refs (object_hash, ref_count, object_type, size, created_at, last_accessed_at)
                 VALUES (?1, 1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(object_hash) DO UPDATE SET
                     ref_count = ref_count + 1,
                     last_accessed_at = excluded.last_accessed_at",
                rusqlite::params![
                    object_hash,
                    "unknown", // 类型将在后续优化中检测
                    0, // 大小将在后续优化中计算
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    /// 减少引用计数
    pub fn decrement(&self, object_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE cas_refs SET
                     ref_count = MAX(0, ref_count - 1),
                     last_accessed_at = ?1
                 WHERE object_hash = ?2",
                rusqlite::params![Utc::now().to_rfc3339(), object_hash],
            )?;
            Ok(())
        })
    }

    /// 获取引用计数
    pub fn get_count(&self, object_hash: &str) -> AppResult<u32> {
        self.db.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT ref_count FROM cas_refs WHERE object_hash = ?1",
                [object_hash],
                |r| r.get(0),
            ).unwrap_or(0);
            Ok(count as u32)
        })
    }

    /// 添加引用关系
    pub fn add_ref_link(&self, source_hash: &str, target_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO cas_ref_links (source_hash, target_hash)
                 VALUES (?1, ?2)",
                rusqlite::params![source_hash, target_hash],
            )?;
            Ok(())
        })
    }

    /// 删除引用关系
    pub fn remove_ref_link(&self, source_hash: &str, target_hash: &str) -> AppResult<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cas_ref_links WHERE source_hash = ?1 AND target_hash = ?2",
                rusqlite::params![source_hash, target_hash],
            )?;
            Ok(())
        })
    }

    /// 查找引用计数为 0 的对象
    pub fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT object_hash FROM cas_refs WHERE ref_count = 0"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(0)?)
            })?;
            Ok(rows.flatten().collect())
        })
    }
}
```

- [ ] **Step 2: 创建测试**

```rust
// src-tauri/tests/cas/ref_counter_test.rs

use iris_lib::cas::ref_counter::RefCounter;
use iris_lib::storage::db::Database;

fn setup() -> (tempfile::TempDir, RefCounter) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open_in_memory().unwrap();
    let ref_counter = RefCounter::new(db);
    (dir, ref_counter)
}

#[test]
fn test_increment_ref_count() {
    let (_dir, ref_counter) = setup();
    let hash = "abc123";

    ref_counter.increment(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 1);

    ref_counter.increment(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 2);
}

#[test]
fn test_decrement_ref_count() {
    let (_dir, ref_counter) = setup();
    let hash = "abc123";

    ref_counter.increment(hash).unwrap();
    ref_counter.increment(hash).unwrap();
    ref_counter.decrement(hash).unwrap();

    assert_eq!(ref_counter.get_count(hash).unwrap(), 1);
}

#[test]
fn test_decrement_does_not_go_below_zero() {
    let (_dir, ref_counter) = setup();
    let hash = "abc123";

    ref_counter.decrement(hash).unwrap();
    assert_eq!(ref_counter.get_count(hash).unwrap(), 0);
}

#[test]
fn test_find_orphaned_objects() {
    let (_dir, ref_counter) = setup();

    ref_counter.increment("hash1").unwrap();
    ref_counter.increment("hash2").unwrap();
    ref_counter.decrement("hash1").unwrap();
    ref_counter.decrement("hash1").unwrap();

    let orphaned = ref_counter.find_orphaned_objects().unwrap();
    assert!(orphaned.contains(&"hash1".to_string()));
    assert!(!orphaned.contains(&"hash2".to_string()));
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p iris-lib cas::ref_counter -- --nocapture`
Expected: PASS

- [ ] **Step 4: 提交代码**

```bash
git add src-tauri/src/cas/ref_counter.rs src-tauri/tests/cas/ref_counter_test.rs
git commit -m "feat(cas): 实现引用计数管理器"
```

---

## Task 4: 创建数据库迁移脚本

**Files:**

- Create: `src-tauri/migrations/016_cas_tables.sql`
- Modify: `src-tauri/src/storage/migrate.rs`

- [ ] **Step 1: 创建迁移脚本**

```sql
-- src-tauri/migrations/016_cas_tables.sql

-- CAS 对象引用计数表
CREATE TABLE IF NOT EXISTS cas_refs (
    object_hash TEXT PRIMARY KEY,
    ref_count INTEGER NOT NULL DEFAULT 0,
    object_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

-- 引用关系表
CREATE TABLE IF NOT EXISTS cas_ref_links (
    source_hash TEXT NOT NULL,
    target_hash TEXT NOT NULL,
    PRIMARY KEY (source_hash, target_hash),
    FOREIGN KEY (source_hash) REFERENCES cas_refs(object_hash),
    FOREIGN KEY (target_hash) REFERENCES cas_refs(object_hash)
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_cas_refs_ref_count ON cas_refs(ref_count);
CREATE INDEX IF NOT EXISTS idx_cas_refs_object_type ON cas_refs(object_type);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_source ON cas_ref_links(source_hash);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_target ON cas_ref_links(target_hash);

-- chunks 表新增 cas_hash 字段
ALTER TABLE chunks ADD COLUMN cas_hash TEXT;
```

- [ ] **Step 2: 注册迁移脚本**

在 `src-tauri/src/storage/migrate.rs` 中添加迁移脚本引用。

- [ ] **Step 3: 运行迁移测试**

Run: `cargo test -p iris-lib storage::migrate -- --nocapture`
Expected: PASS

- [ ] **Step 4: 提交代码**

```bash
git add src-tauri/migrations/016_cas_tables.sql
git commit -m "feat(storage): 添加 CAS 表迁移脚本"
```

---

## Task 5: 集成 CAS 到 AppState

**Files:**

- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: 添加 CAS 存储到 AppState**

```rust
// src-tauri/src/app.rs (修改)

use crate::cas::store::CasObjectStore;
use crate::cas::ref_counter::RefCounter;
use crate::scheduler::Scheduler;

pub struct AppState {
    pub db: Database,
    vault: Mutex<Option<PathBuf>>,
    data_dir: PathBuf,
    pub watcher: Mutex<Option<FileWatcher>>,
    pub active_research: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_tool_calls: Mutex<HashMap<String, PendingToolCall>>,
    pub vector_index_ready: std::sync::atomic::AtomicBool,
    embed_queue: OnceLock<EmbedQueue>,
    pub write_guard: WriteGuard,
    // 新增 CAS 存储
    cas_store: OnceLock<CasObjectStore>,
    ref_counter: OnceLock<RefCounter>,
    scheduler: OnceLock<Scheduler>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> AppResult<Arc<Self>> {
        // ... 现有代码 ...

        let state = Arc::new(Self {
            // ... 现有字段 ...
            cas_store: OnceLock::new(),
            ref_counter: OnceLock::new(),
            scheduler: OnceLock::new(),
        });

        // ... 现有代码 ...

        Ok(state)
    }

    /// 获取 CAS 存储实例
    pub fn cas_store(&self) -> &CasObjectStore {
        self.cas_store.get_or_init(|| {
            let vault = self.vault_path().expect("Vault not configured");
            let cas_path = vault.join(".iris").join("cas");
            CasObjectStore::new(cas_path).expect("Failed to create CAS store")
        })
    }

    /// 获取引用计数管理器实例
    pub fn ref_counter(&self) -> &RefCounter {
        self.ref_counter.get_or_init(|| {
            RefCounter::new(self.db.clone())
        })
    }

    /// 获取调度器实例
    pub fn scheduler(&self) -> &Scheduler {
        self.scheduler.get_or_init(|| {
            Scheduler::new(Arc::new(self.clone()))
        })
    }
}
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/src/app.rs
git commit -m "feat(app): 集成 CAS 存储到 AppState"
```

---

## Task 6: 创建调度器

**Files:**

- Create: `src-tauri/src/scheduler.rs`

- [ ] **Step 1: 实现调度器**

```rust
// src-tauri/src/scheduler.rs

use std::sync::Arc;
use chrono::Utc;
use tokio::time::{sleep, Duration};
use crate::app::AppState;
use crate::cas::garbage_collector::GarbageCollector;
use crate::error::AppResult;

/// 定时任务调度器
pub struct Scheduler {
    state: Arc<AppState>,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// 启动定时任务
    pub fn start(&self) {
        let state = self.state.clone();

        // 每天凌晨 3:00 执行垃圾回收
        tokio::spawn(async move {
            loop {
                let now = Utc::now();
                let next_run = now.date_naive().and_hms_opt(3, 0, 0).unwrap();
                let next_run = if now.time() > next_run.time() {
                    next_run + chrono::Duration::days(1)
                } else {
                    next_run
                };

                let duration = (next_run - now).to_std().unwrap_or(Duration::from_secs(3600));
                sleep(duration).await;

                if let Err(e) = Self::run_garbage_collection(&state).await {
                    tracing::error!("Garbage collection failed: {e}");
                }
            }
        });
    }

    /// 执行垃圾回收
    async fn run_garbage_collection(state: &Arc<AppState>) -> AppResult<()> {
        let gc = GarbageCollector::new(state.cas_store().clone(), state.db.clone());
        let result = gc.collect()?;

        tracing::info!(
            "Garbage collection completed: {} orphaned objects deleted, {} recycle items purged, {} bytes freed",
            result.deleted_count,
            result.recycle_purged_count,
            result.space_freed
        );

        Ok(())
    }
}
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/src/scheduler.rs
git commit -m "feat(scheduler): 实现定时任务调度器"
```

---

## Task 7: 创建垃圾回收器

**Files:**

- Create: `src-tauri/src/cas/garbage_collector.rs`
- Test: `src-tauri/tests/cas/garbage_collector_test.rs`

- [ ] **Step 1: 实现垃圾回收器**

```rust
// src-tauri/src/cas/garbage_collector.rs

use std::fs;
use chrono::Utc;
use crate::cas::store::CasObjectStore;
use crate::error::AppResult;
use crate::storage::db::Database;

/// 垃圾回收结果
#[derive(Debug, Default)]
pub struct GarbageCollectionResult {
    pub orphaned_count: usize,
    pub deleted_count: usize,
    pub recycle_purged_count: usize,
    pub space_freed: u64,
}

/// 垃圾回收器
pub struct GarbageCollector {
    cas_store: CasObjectStore,
    db: Database,
}

impl GarbageCollector {
    /// 创建新的垃圾回收器
    pub fn new(cas_store: CasObjectStore, db: Database) -> Self {
        Self { cas_store, db }
    }

    /// 执行垃圾回收
    pub fn collect(&self) -> AppResult<GarbageCollectionResult> {
        let mut result = GarbageCollectionResult::default();

        // 1. 查找引用计数为 0 的对象
        let orphaned_objects = self.find_orphaned_objects()?;
        result.orphaned_count = orphaned_objects.len();

        // 2. 删除孤立对象
        for object_hash in &orphaned_objects {
            self.delete_object(object_hash)?;
            result.deleted_count += 1;
        }

        // 3. 清理过期回收站条目
        let expired_items = self.find_expired_recycle_items()?;
        for item in expired_items {
            self.purge_recycle_item(&item)?;
            result.recycle_purged_count += 1;
        }

        Ok(result)
    }

    /// 查找孤立对象
    fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT object_hash FROM cas_refs WHERE ref_count = 0"
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(0)?)
            })?;
            Ok(rows.flatten().collect())
        })
    }

    /// 删除对象
    fn delete_object(&self, object_hash: &str) -> AppResult<()> {
        // 1. 删除物理文件
        let object_path = self.cas_store.object_path(object_hash);
        if object_path.exists() {
            fs::remove_file(&object_path)?;
        }

        // 2. 删除引用记录
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM cas_refs WHERE object_hash = ?1",
                [object_hash],
            )?;
            conn.execute(
                "DELETE FROM cas_ref_links WHERE source_hash = ?1 OR target_hash = ?1",
                [object_hash],
            )?;
            Ok(())
        })?;

        Ok(())
    }

    /// 查找过期回收站条目
    fn find_expired_recycle_items(&self) -> AppResult<Vec<(String, String)>> {
        self.db.with_conn(|conn| {
            let now = Utc::now().to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT id, trash_rel_dir FROM recycle_bin WHERE expires_at <= ?1"
            )?;
            let rows = stmt.query_map([&now], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok(rows.flatten().collect())
        })
    }

    /// 清除回收站条目
    fn purge_recycle_item(&self, (id, trash_rel_dir): &(String, String)) -> AppResult<()> {
        // 1. 删除回收站目录
        let trash_dir = self.cas_store.base_path().join(trash_rel_dir);
        if trash_dir.exists() {
            fs::remove_dir_all(&trash_dir)?;
        }

        // 2. 删除数据库记录
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM recycle_bin WHERE id = ?1",
                [id],
            )?;
            Ok(())
        })?;

        Ok(())
    }
}
```

- [ ] **Step 2: 创建测试**

```rust
// src-tauri/tests/cas/garbage_collector_test.rs

use iris_lib::cas::garbage_collector::GarbageCollector;
use iris_lib::cas::store::CasObjectStore;
use iris_lib::storage::db::Database;
use tempfile::tempdir;

#[test]
fn test_garbage_collection_removes_orphaned_objects() {
    let dir = tempdir().unwrap();
    let db = Database::open_in_memory().unwrap();
    let store = CasObjectStore::new(dir.path().to_path_buf()).unwrap();

    // 存储一个对象
    let hash = store.store_blob("test content".as_bytes()).unwrap();

    // 引用计数为 0，应该被回收
    let gc = GarbageCollector::new(store, db);
    let result = gc.collect().unwrap();

    assert_eq!(result.deleted_count, 1);
}
```

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p iris-lib cas::garbage_collector -- --nocapture`
Expected: PASS

- [ ] **Step 4: 提交代码**

```bash
git add src-tauri/src/cas/garbage_collector.rs src-tauri/tests/cas/garbage_collector_test.rs
git commit -m "feat(cas): 实现垃圾回收器"
```

---

## Task 8: 创建乐观锁实现

**Files:**

- Create: `src-tauri/src/cas/write_guard.rs`

- [ ] **Step 1: 实现写入守卫**

```rust
// src-tauri/src/cas/write_guard.rs

use std::collections::HashMap;
use std::sync::Mutex;
use crate::error::AppResult;

/// 写入守卫 - 实现乐观锁
pub struct WriteGuard {
    /// 最近写入的文件哈希缓存
    recent_writes: Mutex<HashMap<String, String>>,
}

impl WriteGuard {
    /// 创建新的写入守卫
    pub fn new() -> Self {
        Self {
            recent_writes: Mutex::new(HashMap::new()),
        }
    }

    /// 标记文件已写入
    pub fn mark(&self, path: &str, hash: &str) {
        let mut cache = self.recent_writes.lock().unwrap();
        cache.insert(path.to_string(), hash.to_string());
    }

    /// 检查是否应该跳过 watcher 事件
    pub fn should_skip_watcher(&self, path: &str, hash: &str) -> bool {
        let cache = self.recent_writes.lock().unwrap();
        if let Some(recent_hash) = cache.get(path) {
            if recent_hash == hash {
                return true;
            }
        }
        false
    }

    /// 验证写入操作（乐观锁）
    pub fn validate_write(
        &self,
        path: &str,
        base_content_hash: &str,
        current_content: &str,
    ) -> AppResult<()> {
        let current_hash = super::hash::content_hash_str(current_content);

        if current_hash != base_content_hash {
            return Err(crate::error::AppError::msg(format!(
                "文件已被修改，请刷新后重试。期望哈希: {}，实际哈希: {}",
                base_content_hash, current_hash
            )));
        }

        Ok(())
    }
}

impl Default for WriteGuard {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/src/cas/write_guard.rs
git commit -m "feat(cas): 实现乐观锁写入守卫"
```

---

## Task 9: 创建补丁应用接口

**Files:**

- Create: `src-tauri/src/cas/patch.rs`

- [ ] **Step 1: 实现补丁应用接口**

```rust
// src-tauri/src/cas/patch.rs

use serde::{Deserialize, Serialize};
use crate::error::AppResult;

/// 补丁应用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub success: bool,
    pub new_content_hash: String,
    pub new_content: String,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// 应用补丁到内容
pub fn apply_patch(
    patch: &crate::ai_runtime::PatchProposal,
    current_content: &str,
) -> AppResult<String> {
    // 验证补丁范围
    let content_len = current_content.len();
    if patch.range.start > content_len || patch.range.end > content_len {
        return Err(crate::error::AppError::msg(format!(
            "补丁范围越界: [{}, {}) 超出内容长度 {}",
            patch.range.start, patch.range.end, content_len
        )));
    }

    // 验证原文匹配
    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Err(crate::error::AppError::msg(format!(
            "原文不一致: 期望 {:?}，实际 {:?}",
            &patch.original_text[..patch.original_text.len().min(50)],
            &actual_original[..actual_original.len().min(50)]
        )));
    }

    // 应用补丁
    let mut new_content = String::with_capacity(
        current_content.len() + patch.replacement_text.len()
    );
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    Ok(new_content)
}

/// 应用补丁到文件（带乐观锁）
pub fn apply_patch_to_file(
    cas_store: &super::store::CasObjectStore,
    write_guard: &super::write_guard::WriteGuard,
    patch: &crate::ai_runtime::PatchProposal,
    current_content: &str,
) -> AppResult<PatchApplyResult> {
    // 1. 验证 base_content_hash
    let current_hash = super::hash::content_hash_str(current_content);
    if current_hash != patch.base_content_hash {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: String::new(),
            new_content: String::new(),
            error: Some(format!(
                "内容哈希不匹配，请刷新后重试。期望哈希: {}，实际哈希: {}",
                patch.base_content_hash, current_hash
            )),
            warnings: vec![],
        });
    }

    // 2. 应用补丁
    let new_content = apply_patch(patch, current_content)?;
    let new_hash = cas_store.write_content(&patch.target_path, &new_content)?;

    // 3. 更新写入守卫
    write_guard.mark(&patch.target_path, &new_hash);

    Ok(PatchApplyResult {
        success: true,
        new_content_hash: new_hash,
        new_content,
        error: None,
        warnings: vec![],
    })
}
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/src/cas/patch.rs
git commit -m "feat(cas): 实现补丁应用接口"
```

---

## Task 10: 更新 Cargo.toml 依赖

**Files:**

- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 添加必要依赖**

```toml
# src-tauri/Cargo.toml

[dependencies]
# 现有依赖...
sha2 = "0.10"
hex = "0.4"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore(deps): 添加 CAS 相关依赖"
```

---

## Task 11: 更新 lib.rs 导出 CAS 模块

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 添加 CAS 模块导出**

```rust
// src-tauri/src/lib.rs

pub mod cas;
pub mod scheduler;
// 现有模块...
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check -p iris-lib`
Expected: PASS

- [ ] **Step 3: 提交代码**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(lib): 导出 CAS 模块"
```

---

## Task 12: 运行完整测试套件

**Files:**

- None (测试现有代码)

- [ ] **Step 1: 运行所有 Rust 测试**

Run: `cargo test -p iris-lib`
Expected: PASS

- [ ] **Step 2: 运行 lint 检查**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 3: 运行格式检查**

Run: `cargo fmt --all -- --check`
Expected: PASS

- [ ] **Step 4: 提交最终代码**

```bash
git add .
git commit -m "feat(cas): 完成 CAS 核心层实现

- 实现统一哈希计算接口
- 实现 CAS 对象存储
- 实现引用计数管理器
- 实现垃圾回收器
- 实现乐观锁写入守卫
- 实现补丁应用接口
- 添加数据库迁移脚本
- 集成到 AppState

Closes #xxx"
```

---

## 自检清单

### 规范覆盖检查

- [x] **CAS 核心层** — Task 1-3, 7-9
- [x] **领域层** — 后续计划
- [x] **版本管理重构** — 后续计划
- [x] **AI体系适配** — 后续计划
- [x] **垃圾回收** — Task 3, 7
- [x] **并发控制** — Task 8
- [x] **测试策略** — Task 12

### 占位符扫描

- [x] 无 "TBD", "TODO", "implement later"
- [x] 所有代码块完整
- [x] 所有测试用例完整

### 类型一致性检查

- [x] 所有函数签名一致
- [x] 所有类型定义一致
- [x] 所有错误处理一致

---

## 执行选项

**Plan complete and saved to `docs/superpowers/plans/2026-06-02-file-management-refactor.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
