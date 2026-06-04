# 文件管理系统深度重构设计文档

> 本文档描述 Iris 项目文件管理系统的深度重构方案，引入内容寻址存储 (CAS) 和领域驱动设计 (DDD) 理论，全面提升文件创建、管理、删除、隔离、生命周期管理、历史版本追踪、数据统计等方面的能力。

**文档状态**：已批准  
**创建日期**：2026-06-02  
**最后更新**：2026-06-02

---

## 一、架构总览

### 1.1 核心架构：分层架构 + CAS 核心层

```
┌─────────────────────────────────────────────────────────────┐
│                    命令层 (Commands)                         │
│  file_create, file_delete, version_save, ai_send_message... │
├─────────────────────────────────────────────────────────────┤
│                    领域层 (Domain)                           │
│  ┌─────────┐ ┌──────────┐ ┌─────────────┐ ┌─────────────┐  │
│  │  File    │ │ Version  │ │ RecycleBin  │ │ AiContext   │  │
│  │ 聚合根   │ │ 聚合根   │ │  聚合根      │ │  聚合根      │  │
│  └─────────┘ └──────────┘ └─────────────┘ └─────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                    CAS 核心层                                │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐   │
│  │ ObjectStore │ │ RefCounter  │ │ GarbageCollector    │   │
│  │ 内容寻址存储 │ │ 引用计数器   │ │ 垃圾回收器          │   │
│  └─────────────┘ └─────────────┘ └─────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│                    存储层 (Storage)                          │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐   │
│  │ SQLite      │ │ FileSystem  │ │ CredentialManager   │   │
│  │ 索引+元数据  │ │ .md文件     │ │ API Key             │   │
│  └─────────────┘ └─────────────┘ └─────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 关键设计原则

1. **`.md` 文件仍是权威数据源** — CAS 对象库只存储版本快照和索引数据
2. **内容寻址** — 所有内容块用 SHA-256 哈希作为唯一标识
3. **引用计数** — 每个 CAS 对象维护引用计数，支持垃圾回收
4. **乐观锁** — 所有写入操作通过 `base_content_hash` 验证
5. **AI体系兼容** — 提供统一的哈希计算接口，AI体系通过该接口访问

### 1.3 理论框架

| 理论               | 应用场景               | 核心优势                           |
| ------------------ | ---------------------- | ---------------------------------- |
| 内容寻址存储 (CAS) | 版本快照、内容去重     | 天然去重、完整性校验、高效版本管理 |
| 领域驱动设计 (DDD) | 聚合根划分、不变量约束 | 边界清晰、职责单一、易于测试       |
| 乐观锁             | 并发写入控制           | 简单高效、符合单用户场景           |

---

## 二、CAS 对象库设计

### 2.1 对象模型

```
┌─────────────────────────────────────────────────────────────┐
│                    CAS Object Store                          │
├─────────────────────────────────────────────────────────────┤
│  Object Types:                                              │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐   │
│  │ BlobObject  │ │ TreeObject  │ │ CommitObject        │   │
│  │ 内容块      │ │ 目录树      │ │ 版本提交            │   │
│  │ (文件内容)   │ │ (元数据)    │ │ (版本快照)          │   │
│  └─────────────┘ └─────────────┘ └─────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  Storage Layout:                                            │
│  .iris/cas/                                                 │
│  ├── objects/                                               │
│  │   ├── ab/                                               │
│  │   │   └── cdef1234... (blob)                            │
│  │   ├── cd/                                               │
│  │   │   └── ef567890... (tree)                            │
│  │   └── ef/                                               │
│  │       └── 12345678... (commit)                          │
│  ├── refs/                                                  │
│  │   ├── files/                                            │
│  │   │   └── <file_id> → commit_hash                       │
│  │   └── versions/                                         │
│  │       └── <version_id> → commit_hash                    │
│  └── gc/                                                    │
│      └── refs_count.db (引用计数)                           │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 对象类型定义

#### BlobObject（内容块）

```rust
struct BlobObject {
    hash: String,        // SHA-256 内容哈希
    content: Vec<u8>,    // 原始内容
    size: u64,           // 内容大小
    ref_count: u32,      // 引用计数
    created_at: DateTime<Utc>,
}
```

#### TreeObject（目录树）

```rust
struct TreeObject {
    hash: String,        // 树哈希
    entries: Vec<TreeEntry>,
    ref_count: u32,
    created_at: DateTime<Utc>,
}

struct TreeEntry {
    name: String,        // 文件名或目录名
    object_hash: String, // 引用的对象哈希
    object_type: ObjectType,
    mode: String,        // 文件权限
}
```

#### CommitObject（版本提交）

```rust
struct CommitObject {
    hash: String,            // 提交哈希
    tree_hash: String,       // 根树哈希
    parent_hash: Option<String>, // 父提交哈希
    author: String,          // 作者
    message: String,         // 提交消息
    metadata: CommitMetadata,
    created_at: DateTime<Utc>,
}

struct CommitMetadata {
    file_id: i64,
    version_no: String,
    label: Option<String>,
    kind: VersionKind,
    word_count: i64,
    is_finalized: bool,
}
```

### 2.3 与现有系统的映射

| 现有概念   | CAS 对象类型 | 说明                     |
| ---------- | ------------ | ------------------------ |
| 文件内容   | BlobObject   | `.md` 文件内容           |
| 版本快照   | CommitObject | 包含元数据和树引用       |
| 文件元数据 | TreeObject   | 包含 frontmatter、标签等 |
| chunks     | BlobObject   | 分块内容，用于检索       |
| embeddings | BlobObject   | 向量嵌入                 |

### 2.4 哈希算法统一

**当前问题：**

- `writing_workflow.rs` 使用独立的 SHA-256 实现
- `organize_workflow.rs` 使用独立的 SHA-256 实现
- `document_workflow.rs` 使用独立的 SHA-256 实现

**解决方案：** 提供统一的哈希计算接口

```rust
// src-tauri/src/cas/hash.rs

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
```

**AI体系适配：**

- `writing_workflow.rs` 的 `compute_content_hash()` 改为调用 `cas::hash::content_hash_str()`
- `organize_workflow.rs` 的 `FileMetadata.content_hash` 改为使用 CAS 哈希
- `document_workflow.rs` 的 `content_hash_for_input()` 改为调用 CAS 哈希

---

## 三、领域层设计

### 3.1 聚合根划分

```
┌─────────────────────────────────────────────────────────────┐
│                    领域层 (Domain Layer)                      │
├─────────────────────────────────────────────────────────────┤
│  File Aggregate                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ File (聚合根)                                        │   │
│  │ - id: i64                                           │   │
│  │ - path: String                                      │   │
│  │ - title: String                                     │   │
│  │ - content_hash: String (CAS对象哈希)                 │   │
│  │ - word_count: i64                                   │   │
│  │ - created_at: DateTime                              │   │
│  │ - updated_at: DateTime                              │   │
│  │                                                     │   │
│  │ 方法:                                               │   │
│  │ - create() -> File                                  │   │
│  │ - update_content(content) -> Result<()>             │   │
│  │ - rename(new_path) -> Result<()>                    │   │
│  │ - delete() -> Result<()>                            │   │
│  │ - add_tag(tag) -> Result<()>                        │   │
│  │ - remove_tag(tag) -> Result<()>                     │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  Version Aggregate                                          │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Version (聚合根)                                     │   │
│  │ - id: i64                                           │   │
│  │ - file_id: i64                                      │   │
│  │ - version_no: String                                │   │
│  │ - commit_hash: String (CAS提交哈希)                  │   │
│  │ - label: Option<String>                             │   │
│  │ - kind: VersionKind                                 │   │
│  │ - is_finalized: bool                                │   │
│  │ - created_at: DateTime                              │   │
│  │                                                     │   │
│  │ 方法:                                               │   │
│  │ - create(file_id, content, kind) -> Version         │   │
│  │ - restore() -> Result<String>                       │   │
│  │ - delete() -> Result<()>                            │   │
│  │ - finalize(label) -> Result<()>                     │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  RecycleBin Aggregate                                       │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ RecycleBinItem (聚合根)                              │   │
│  │ - id: String (UUID)                                 │   │
│  │ - original_path: String                             │   │
│  │ - title: String                                     │   │
│  │ - deleted_at: DateTime                              │   │
│  │ - expires_at: DateTime                              │   │
│  │ - trash_rel_dir: String                             │   │
│  │                                                     │   │
│  │ 方法:                                               │   │
│  │ - trash(file) -> RecycleBinItem                     │   │
│  │ - restore() -> Result<File>                         │   │
│  │ - purge() -> Result<()>                             │   │
│  │ - is_expired() -> bool                              │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  AiContext Aggregate                                        │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ AiContext (聚合根)                                   │   │
│  │ - request_id: String                                │   │
│  │ - scene: AiScene                                    │   │
│  │ - note_path: Option<String>                         │   │
│  │ - content_hash: String (当前文档哈希)                 │   │
│  │ - packets: Vec<ContextPacket>                       │   │
│  │ - session_id: Option<i64>                           │   │
│  │                                                     │   │
│  │ 方法:                                               │   │
│  │ - assemble_context() -> AssembledContext             │   │
│  │ - validate_patch(patch) -> Result<()>               │   │
│  │ - apply_patch(patch) -> Result<String>              │   │
│  │ - create_version(content, kind) -> Result<Version>  │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 聚合间关系

```
File ──1:N──> Version
File ──1:1──> RecycleBinItem (删除后)
AiContext ──N:1──> File
AiContext ──N:M──> Version
```

### 3.3 不变量约束

#### File 聚合不变量

1. `path` 必须唯一
2. `content_hash` 必须是有效的 CAS 对象哈希
3. `path` 不能以 `.iris/` 开头（用户笔记路径）

#### Version 聚合不变量

1. `file_id` 必须引用有效的 File
2. `commit_hash` 必须是有效的 CAS 提交哈希
3. `version_no` 在同一 `file_id` 下必须唯一
4. `is_finalized` 为 true 时不能删除

#### RecycleBinItem 聚合不变量

1. `original_path` 必须是有效的用户笔记路径
2. `expires_at` 必须晚于 `deleted_at`
3. 恢复时目标路径不能已存在

#### AiContext 聚合不变量

1. `content_hash` 必须与当前文档内容匹配
2. `packets` 中的证据必须在有效期内
3. 应用补丁前必须验证 `base_content_hash`

---

## 四、AI体系适配设计

### 4.1 哈希计算统一

#### 当前问题

AI体系中有多个独立的哈希计算实现：

- `writing_workflow.rs:60-64` — `compute_content_hash()`
- `organize_workflow.rs:268` — `FileMetadata.content_hash`
- `document_workflow.rs:702-708` — `content_hash_for_input()`
- `citation_workflow.rs:20-30` — `generate_claim_id()`

#### 解决方案

引入统一的 CAS 哈希接口：

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
```

#### AI体系适配点

| 文件                   | 当前实现                    | 适配方案                                 |
| ---------------------- | --------------------------- | ---------------------------------------- |
| `writing_workflow.rs`  | `compute_content_hash()`    | 改为调用 `cas::hash::content_hash_str()` |
| `organize_workflow.rs` | `FileMetadata.content_hash` | 改为使用 CAS 哈希                        |
| `document_workflow.rs` | `content_hash_for_input()`  | 改为调用 CAS 哈希                        |
| `citation_workflow.rs` | `generate_claim_id()`       | 保持不变（ID生成不需要CAS）              |

### 4.2 文件读取层抽象

#### 当前问题

AI体系直接读取文件系统：

- `tool_dispatch.rs:259-277` — `read_note()` 直接读取文件
- `retrieval_broker.rs:196-255` — `search_vector_chunks()` 通过SQLite查询

#### 解决方案

引入统一的文件读取接口：

```rust
// src-tauri/src/cas/store.rs

/// CAS 对象存储
pub struct CasObjectStore {
    base_path: PathBuf,
    db: Database,
}

impl CasObjectStore {
    /// 读取文件内容（优先从CAS，回退到文件系统）
    pub fn read_content(&self, path: &str) -> AppResult<String> {
        // 1. 尝试从CAS读取
        if let Some(content) = self.read_from_cas(path)? {
            return Ok(content);
        }
        // 2. 回退到文件系统
        self.read_from_filesystem(path)
    }

    /// 写入文件内容（写入CAS + 更新文件系统）
    pub fn write_content(&self, path: &str, content: &str) -> AppResult<String> {
        let hash = content_hash_str(content);
        self.store_blob(&hash, content.as_bytes())?;
        self.write_to_filesystem(path, content)?;
        Ok(hash)
    }
}
```

#### AI体系适配点

| 文件                  | 当前实现                 | 适配方案                            |
| --------------------- | ------------------------ | ----------------------------------- |
| `tool_dispatch.rs`    | `read_note()` 直接读取   | 改为调用 `cas_store.read_content()` |
| `tool_dispatch.rs`    | `get_outline()` 直接读取 | 改为调用 `cas_store.read_content()` |
| `retrieval_broker.rs` | 通过SQLite查询           | 保持不变（索引层不需要改）          |

### 4.3 补丁应用层适配

#### 当前问题

补丁应用后直接写入文件系统：

- `writing_workflow.rs:336-349` — `apply_patch()` 返回新内容，由调用方写入
- `document_workflow.rs:727-844` — `build_heuristic_document_patches()` 创建补丁

#### 解决方案

引入统一的补丁应用接口：

```rust
// src-tauri/src/cas/patch.rs

/// 补丁应用结果
pub struct PatchApplyResult {
    pub success: bool,
    pub new_content_hash: String,
    pub new_content: String,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// 应用补丁到文件
pub fn apply_patch_to_file(
    cas_store: &CasObjectStore,
    patch: &PatchProposal,
    current_content: &str,
) -> AppResult<PatchApplyResult> {
    // 1. 验证 base_content_hash
    let current_hash = content_hash_str(current_content);
    if current_hash != patch.base_content_hash {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: String::new(),
            new_content: String::new(),
            error: Some("内容哈希不匹配".to_string()),
            warnings: vec![],
        });
    }

    // 2. 应用补丁
    let new_content = apply_patch(patch, current_content)?;
    let new_hash = cas_store.write_content(&patch.target_path, &new_content)?;

    Ok(PatchApplyResult {
        success: true,
        new_content_hash: new_hash,
        new_content,
        error: None,
        warnings: vec![],
    })
}
```

#### AI体系适配点

| 文件                   | 当前实现                             | 适配方案                                     |
| ---------------------- | ------------------------------------ | -------------------------------------------- |
| `writing_workflow.rs`  | `apply_patch()`                      | 改为调用 `cas::patch::apply_patch_to_file()` |
| `document_workflow.rs` | `build_heuristic_document_patches()` | 保持不变（只创建补丁，不应用）               |

### 4.4 检索层适配

#### 当前问题

检索层直接查询SQLite表：

- `retrieval_broker.rs:196-255` — `search_vector_chunks()` 查询chunks表
- `retrieval_broker.rs:382-434` — `search_fts()` 查询FTS5

#### 解决方案

保持检索层不变，但调整chunks表结构以支持CAS对象引用：

```sql
-- 新增 chunks 表的 CAS 对象引用
ALTER TABLE chunks ADD COLUMN cas_hash TEXT;
-- 迁移时填充 cas_hash
UPDATE chunks SET cas_hash = (
    SELECT hash FROM cas_objects
    WHERE cas_objects.content = chunks.content
);
```

#### AI体系适配点

| 文件                  | 当前实现         | 适配方案                   |
| --------------------- | ---------------- | -------------------------- |
| `retrieval_broker.rs` | 直接查询chunks表 | 保持不变（索引层不需要改） |
| `indexer/scan.rs`     | 写入chunks表     | 增加 `cas_hash` 字段填充   |

---

## 五、版本管理重构设计

### 5.1 版本存储模型重构

#### 当前实现

```
.iris/versions/
└── <file_id>/
    ├── 20260525143052123.md
    └── ...
```

#### 新实现（CAS 对象库）

```
.iris/cas/
├── objects/
│   ├── ab/
│   │   └── cdef1234... (blob - 文件内容)
│   ├── cd/
│   │   └── ef567890... (tree - 文件元数据)
│   └── ef/
│       └── 12345678... (commit - 版本提交)
├── refs/
│   ├── files/
│   │   └── <file_id> → commit_hash
│   └── versions/
│       └── <version_id> → commit_hash
└── gc/
    └── refs_count.db (引用计数)
```

### 5.2 版本创建流程

#### 当前流程

1. 计算内容哈希
2. 写入 `.iris/versions/<file_id>/<timestamp>.md`
3. 插入 `versions` 表记录

#### 新流程

1. 计算内容哈希（使用 CAS 统一接口）
2. 创建 BlobObject（内容块）
3. 创建 TreeObject（文件元数据）
4. 创建 CommitObject（版本提交）
5. 更新 refs 指向
6. 更新引用计数
7. 插入 `versions` 表记录（引用 CAS 哈希）

```rust
// src-tauri/src/version/mod.rs

/// 创建版本快照（CAS 实现）
pub fn create_snapshot(
    state: &Arc<AppState>,
    path: &str,
    content: &str,
    params: SnapshotParams,
) -> AppResult<Option<VersionEntry>> {
    let vault = state.vault_path()?;
    let cas_store = state.cas_store();

    // 1. 计算内容哈希
    let hash = cas::hash::content_hash_str(content);

    // 2. 检查是否需要创建快照
    let file_id = state.db.with_conn(|conn| {
        conn.query_row("SELECT id FROM files WHERE path = ?1", [path], |r| r.get(0))
            .map_err(|e| AppError::msg(format!("File not indexed: {e}")))
    })?;

    let should = state.db.with_conn(|conn| {
        let (latest, last_auto_idle_at) = load_snapshot_context(conn, file_id)?;
        Ok(policy::should_create_snapshot(&SnapshotDecisionInput {
            kind: params.kind,
            content_hash: &hash,
            latest,
            last_auto_idle_at,
            now: Utc::now(),
        }))
    })?;

    if !should {
        return Ok(None);
    }

    // 3. 创建 CAS 对象
    let blob_hash = cas_store.store_blob(content.as_bytes())?;
    let tree_hash = cas_store.store_tree(&TreeObject {
        entries: vec![
            TreeEntry {
                name: "content".to_string(),
                object_hash: blob_hash.clone(),
                object_type: ObjectType::Blob,
                mode: "100644".to_string(),
            },
        ],
    })?;

    let commit_hash = cas_store.store_commit(&CommitObject {
        tree_hash,
        parent_hash: None,
        author: "Iris".to_string(),
        message: format!("Version snapshot: {}", params.kind.as_str()),
        metadata: CommitMetadata {
            file_id,
            version_no: timestamp_version_no(),
            label: params.label.clone(),
            kind: params.kind,
            word_count: content.split_whitespace().count() as i64,
            is_finalized: params.is_finalized,
        },
    })?;

    // 4. 更新引用
    cas_store.update_ref(&format!("versions/{}", file_id), &commit_hash)?;

    // 5. 更新引用计数
    cas_store.increment_ref(&blob_hash)?;
    cas_store.increment_ref(&tree_hash)?;
    cas_store.increment_ref(&commit_hash)?;

    // 6. 插入数据库记录
    let version_no = timestamp_version_no();
    let id = state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO versions (file_id, version_no, label, content_hash, storage_path, word_count, is_finalized, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                file_id,
                version_no,
                params.label,
                hash,
                commit_hash, // 存储 CAS 哈希而不是文件路径
                content.split_whitespace().count() as i64,
                if params.is_finalized { 1 } else { 0 },
                params.kind.as_str(),
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    })?;

    Ok(Some(VersionEntry {
        id,
        file_id,
        version_no,
        label: params.label,
        content_hash: hash,
        word_count: content.split_whitespace().count() as i64,
        is_finalized: params.is_finalized,
        kind: params.kind,
        created_at: Utc::now().to_rfc3339(),
    }))
}
```

### 5.3 版本恢复流程

#### 当前流程

1. 读取 `.iris/versions/<file_id>/<version_no>.md`
2. 写入 `.md` 文件
3. 更新索引

#### 新流程

1. 从 CAS 读取 CommitObject
2. 从 CommitObject 读取 TreeObject
3. 从 TreeObject 读取 BlobObject（内容）
4. 写入 `.md` 文件
5. 更新索引

```rust
/// 恢复版本（CAS 实现）
pub fn version_restore(
    state: &Arc<AppState>,
    version_id: i64,
    current_content: &str,
) -> AppResult<String> {
    let cas_store = state.cas_store();

    // 1. 获取版本信息
    let (commit_hash, path): (String, String) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT storage_path, f.path FROM versions v JOIN files f ON f.id = v.file_id WHERE v.id = ?1",
            [version_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })?;

    // 2. 创建恢复前快照
    let pre_restore = create_snapshot(state, &path, current_content, SnapshotParams::pre_restore())?;
    if pre_restore.is_none() {
        return Err(AppError::msg("恢复前备份未能创建，已取消恢复以保护当前正文"));
    }

    // 3. 从 CAS 读取内容
    let commit = cas_store.read_commit(&commit_hash)?;
    let tree = cas_store.read_tree(&commit.tree_hash)?;
    let content = cas_store.read_blob_content(&tree.entries[0].object_hash)?;

    // 4. 写入文件
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;
    let tmp = abs.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &abs)?;

    // 5. 更新索引
    state.db.with_conn(|conn| {
        crate::indexer::scan::index_file(conn, &vault, &abs)
    })?;

    Ok(content)
}
```

### 5.4 版本清理流程

#### 当前流程

1. 删除 `.iris/versions/<file_id>/<version_no>.md`
2. 删除 `versions` 表记录

#### 新流程

1. 从 CAS 读取 CommitObject
2. 递减引用计数
3. 如果引用计数为 0，标记为可回收
4. 删除 `versions` 表记录
5. 周期性执行垃圾回收

```rust
/// 删除版本（CAS 实现）
pub fn version_delete(state: &Arc<AppState>, version_id: i64) -> AppResult<()> {
    let cas_store = state.cas_store();

    // 1. 获取版本信息
    let (commit_hash,): (String,) = state.db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT storage_path FROM versions WHERE id = ?1",
            [version_id],
            |r| Ok((r.get(0)?,)),
        )?)
    })?;

    // 2. 从 CAS 读取提交对象
    let commit = cas_store.read_commit(&commit_hash)?;
    let tree = cas_store.read_tree(&commit.tree_hash)?;

    // 3. 递减引用计数
    cas_store.decrement_ref(&commit_hash)?;
    cas_store.decrement_ref(&commit.tree_hash)?;
    for entry in &tree.entries {
        cas_store.decrement_ref(&entry.object_hash)?;
    }

    // 4. 删除数据库记录
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM versions WHERE id = ?1", [version_id])?;
        Ok(())
    })?;

    Ok(())
}
```

---

## 六、垃圾回收与引用计数设计

### 6.1 引用计数模型

#### 引用关系图

```
CommitObject (版本提交)
    │
    ├──引用──> TreeObject (文件元数据)
    │              │
    │              ├──引用──> BlobObject (文件内容)
    │              └──引用──> BlobObject (frontmatter)
    │
    └──引用──> BlobObject (提交消息)
```

#### 引用计数表结构

```sql
-- CAS 对象引用计数表
CREATE TABLE cas_refs (
    object_hash TEXT PRIMARY KEY,
    ref_count INTEGER NOT NULL DEFAULT 0,
    object_type TEXT NOT NULL, -- 'blob', 'tree', 'commit'
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

-- 引用关系表（用于追踪引用来源）
CREATE TABLE cas_ref_links (
    source_hash TEXT NOT NULL,
    target_hash TEXT NOT NULL,
    PRIMARY KEY (source_hash, target_hash),
    FOREIGN KEY (source_hash) REFERENCES cas_refs(object_hash),
    FOREIGN KEY (target_hash) REFERENCES cas_refs(object_hash)
);
```

### 6.2 引用计数操作

```rust
// src-tauri/src/cas/ref_counter.rs

/// 引用计数管理器
pub struct RefCounter {
    db: Database,
}

impl RefCounter {
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
                    self.detect_object_type(object_hash)?,
                    self.detect_object_size(object_hash)?,
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
}
```

### 6.3 垃圾回收策略

#### 回收时机

| 时机     | 触发条件      | 回收范围                |
| -------- | ------------- | ----------------------- |
| 应用启动 | 每次启动时    | 清理过期回收站条目      |
| 版本删除 | 删除版本时    | 递减引用计数            |
| 定期执行 | 每天凌晨 3:00 | 清理引用计数为 0 的对象 |
| 手动触发 | 用户请求      | 全量垃圾回收            |

#### 回收算法

```rust
// src-tauri/src/cas/garbage_collector.rs

/// 垃圾回收器
pub struct GarbageCollector {
    cas_store: CasObjectStore,
    ref_counter: RefCounter,
}

impl GarbageCollector {
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
            self.purge_recycle_item(&item.id)?;
            result.recycle_purged_count += 1;
        }

        // 4. 计算释放空间
        result.space_freed = self.calculate_freed_space(&orphaned_objects)?;

        Ok(result)
    }

    /// 查找孤立对象
    fn find_orphaned_objects(&self) -> AppResult<Vec<String>> {
        self.cas_store.db.with_conn(|conn| {
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
        self.cas_store.db.with_conn(|conn| {
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
    fn find_expired_recycle_items(&self) -> AppResult<Vec<RecycleBinItem>> {
        self.cas_store.db.with_conn(|conn| {
            let now = Utc::now().to_rfc3339();
            let mut stmt = conn.prepare(
                "SELECT id, original_path, title, deleted_at, expires_at, trash_rel_dir
                 FROM recycle_bin WHERE expires_at <= ?1"
            )?;
            let rows = stmt.query_map([&now], |row| {
                Ok(RecycleBinItem {
                    id: row.get(0)?,
                    original_path: row.get(1)?,
                    title: row.get(2)?,
                    deleted_at: row.get(3)?,
                    expires_at: row.get(4)?,
                    trash_rel_dir: row.get(5)?,
                })
            })?;
            Ok(rows.flatten().collect())
        })
    }

    /// 清除回收站条目
    fn purge_recycle_item(&self, id: &str) -> AppResult<()> {
        // 1. 获取回收站条目信息
        let item = self.cas_store.db.with_conn(|conn| {
            conn.query_row(
                "SELECT trash_rel_dir FROM recycle_bin WHERE id = ?1",
                [id],
                |r| Ok(r.get::<_, String>(0)?),
            )
        })?;

        // 2. 删除回收站目录
        let trash_dir = self.cas_store.base_path.join(&item);
        if trash_dir.exists() {
            fs::remove_dir_all(&trash_dir)?;
        }

        // 3. 删除数据库记录
        self.cas_store.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM recycle_bin WHERE id = ?1",
                [id],
            )?;
            Ok(())
        })?;

        Ok(())
    }
}

/// 垃圾回收结果
#[derive(Debug, Default)]
pub struct GarbageCollectionResult {
    pub orphaned_count: usize,
    pub deleted_count: usize,
    pub recycle_purged_count: usize,
    pub space_freed: u64,
}
```

### 6.4 定时任务调度

```rust
// src-tauri/src/scheduler.rs

/// 定时任务调度器
pub struct Scheduler {
    state: Arc<AppState>,
}

impl Scheduler {
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
                tokio::time::sleep(duration).await;

                if let Err(e) = Self::run_garbage_collection(&state).await {
                    tracing::error!("Garbage collection failed: {e}");
                }
            }
        });
    }

    /// 执行垃圾回收
    async fn run_garbage_collection(state: &Arc<AppState>) -> AppResult<()> {
        let gc = GarbageCollector::new(state.cas_store().clone());
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

---

## 七、并发控制与乐观锁设计

### 7.1 乐观锁模型

#### 锁定机制

```
┌─────────────────────────────────────────────────────────────┐
│                    乐观锁流程                                 │
├─────────────────────────────────────────────────────────────┤
│  1. 读取文件内容，计算 content_hash                           │
│  2. 用户编辑文件                                             │
│  3. 提交时验证:                                              │
│     - 当前 content_hash == base_content_hash?                │
│     - 如果匹配，允许写入                                     │
│     - 如果不匹配，返回冲突错误                               │
│  4. 写入成功后，更新 content_hash                            │
└─────────────────────────────────────────────────────────────┘
```

#### 冲突检测点

| 操作       | 冲突检测点            | 冲突处理                       |
| ---------- | --------------------- | ------------------------------ |
| 文件写入   | `file_write`          | 返回错误，提示用户刷新         |
| 版本恢复   | `version_restore`     | 创建 pre_restore 快照后恢复    |
| 补丁应用   | `apply_patch_to_file` | 返回错误，提示用户重新生成补丁 |
| 文件重命名 | `file_rename`         | 返回错误，提示用户刷新         |
| 文件删除   | `file_delete`         | 返回错误，提示用户刷新         |

### 7.2 写入锁实现

```rust
// src-tauri/src/cas/write_guard.rs

/// 写入守卫 - 实现乐观锁
pub struct WriteGuard {
    /// 最近写入的文件哈希缓存
    recent_writes: Mutex<HashMap<String, String>>,
}

impl WriteGuard {
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
        let current_hash = cas::hash::content_hash_str(current_content);

        if current_hash != base_content_hash {
            return Err(AppError::msg(format!(
                "文件已被修改，请刷新后重试。期望哈希: {}，实际哈希: {}",
                base_content_hash, current_hash
            )));
        }

        Ok(())
    }
}
```

### 7.3 文件写入流程（带乐观锁）

```rust
// src-tauri/src/commands/file.rs

/// 写入文件（带乐观锁）
#[tauri::command]
pub fn file_write(
    state: State<'_, Arc<AppState>>,
    path: String,
    content: String,
    base_content_hash: Option<String>, // 新增参数
) -> AppResult<FileEntry> {
    if !is_user_note_path(&path) {
        return Err(AppError::msg("只能写入用户笔记，不允许修改内部元数据路径"));
    }

    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &path)?;

    // 乐观锁验证
    if let Some(base_hash) = base_content_hash {
        if abs.exists() {
            let current_content = fs::read_to_string(&abs)?;
            state.write_guard.validate_write(&path, &base_hash, &current_content)?;
        }
    }

    // 写入文件
    let tmp = abs.with_extension("md.tmp");
    fs::write(&tmp, &content)?;
    fs::rename(&tmp, &abs)?;

    // 更新哈希缓存
    let hash = cas::hash::content_hash_str(&content);
    state.write_guard.mark(&path, &hash);

    // 更新索引
    state.db.with_conn(|conn| {
        index_file_with_embed(conn, &vault, &abs, Some(state.inner()))
    })
}
```

### 7.4 补丁应用流程（带乐观锁）

```rust
// src-tauri/src/cas/patch.rs

/// 应用补丁到文件（带乐观锁）
pub fn apply_patch_to_file(
    cas_store: &CasObjectStore,
    write_guard: &WriteGuard,
    patch: &PatchProposal,
    current_content: &str,
) -> AppResult<PatchApplyResult> {
    // 1. 验证 base_content_hash
    let current_hash = cas::hash::content_hash_str(current_content);
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

    // 2. 验证补丁范围
    let content_len = current_content.len();
    if patch.range.start > content_len || patch.range.end > content_len {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: String::new(),
            new_content: String::new(),
            error: Some(format!(
                "补丁范围越界: [{}, {}) 超出内容长度 {}",
                patch.range.start, patch.range.end, content_len
            )),
            warnings: vec![],
        });
    }

    // 3. 验证原文匹配
    let actual_original = &current_content[patch.range.start..patch.range.end];
    if actual_original != patch.original_text {
        return Ok(PatchApplyResult {
            success: false,
            new_content_hash: String::new(),
            new_content: String::new(),
            error: Some(format!(
                "原文不一致: 期望 {:?}，实际 {:?}",
                &patch.original_text[..patch.original_text.len().min(50)],
                &actual_original[..actual_original.len().min(50)]
            )),
            warnings: vec![],
        });
    }

    // 4. 应用补丁
    let mut new_content = String::with_capacity(
        current_content.len() + patch.replacement_text.len()
    );
    new_content.push_str(&current_content[..patch.range.start]);
    new_content.push_str(&patch.replacement_text);
    new_content.push_str(&current_content[patch.range.end..]);

    // 5. 写入文件
    let new_hash = cas_store.write_content(&patch.target_path, &new_content)?;

    // 6. 更新写入守卫
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

### 7.5 冲突解决策略

#### 冲突类型

| 冲突类型   | 场景                     | 解决策略                       |
| ---------- | ------------------------ | ------------------------------ |
| 文件级冲突 | 用户编辑时文件被外部修改 | 返回错误，提示用户刷新         |
| 补丁级冲突 | 补丁应用时文件已被修改   | 返回错误，提示用户重新生成补丁 |
| 版本级冲突 | 版本恢复时文件已被修改   | 创建 pre_restore 快照后恢复    |

#### 冲突处理流程

```
用户操作
    │
    ├──检测到冲突──> 返回错误信息
    │                  │
    │                  ├──提示用户刷新
    │                  └──提供"强制覆盖"选项（可选）
    │
    └──无冲突──> 执行操作
                   │
                   ├──更新 content_hash
                   └──通知其他组件
```

---

## 八、测试策略设计

### 8.1 测试金字塔

```
┌─────────────────────────────────────────────────────────────┐
│                    测试金字塔                                 │
├─────────────────────────────────────────────────────────────┤
│  E2E 测试 (5%)                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ - 完整文件生命周期测试                                │   │
│  │ - AI补丁应用流程测试                                 │   │
│  │ - 版本恢复流程测试                                   │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  集成测试 (25%)                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ - CAS对象存储测试                                    │   │
│  │ - 引用计数测试                                       │   │
│  │ - 垃圾回收测试                                       │   │
│  │ - 版本管理测试                                       │   │
│  │ - 文件系统操作测试                                   │   │
│  └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│  单元测试 (70%)                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ - 哈希计算测试                                       │   │
│  │ - 路径验证测试                                       │   │
│  │ - 补丁验证测试                                       │   │
│  │ - 引用计数操作测试                                   │   │
│  │ - 领域模型测试                                       │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 8.2 测试目录结构

```
src-tauri/src/
├── cas/
│   ├── hash.rs
│   ├── store.rs
│   ├── ref_counter.rs
│   ├── garbage_collector.rs
│   └── mod.rs
├── version/
│   ├── mod.rs
│   ├── kind.rs
│   └── policy.rs
├── commands/
│   ├── file.rs
│   └── version.rs
└── ...

src-tauri/tests/
├── cas/
│   ├── hash_test.rs
│   ├── store_test.rs
│   ├── ref_counter_test.rs
│   └── garbage_collector_test.rs
├── version/
│   ├── version_test.rs
│   └── policy_test.rs
├── commands/
│   ├── file_test.rs
│   └── version_test.rs
└── integration/
    ├── file_lifecycle_test.rs
    ├── version_restore_test.rs
    └── ai_patch_apply_test.rs
```

### 8.3 测试覆盖率目标

| 模块       | 目标覆盖率 | 说明                         |
| ---------- | ---------- | ---------------------------- |
| CAS 核心层 | 90%        | 哈希计算、对象存储、引用计数 |
| 领域层     | 85%        | 聚合根、不变量约束           |
| 命令层     | 80%        | 文件操作、版本管理           |
| AI 适配层  | 75%        | 哈希统一、补丁应用           |
| 集成测试   | 70%        | 关键业务流程                 |
| E2E 测试   | 60%        | 完整生命周期                 |

---

## 九、数据库迁移设计

### 9.1 新增表

```sql
-- CAS 对象引用计数表
CREATE TABLE IF NOT EXISTS cas_refs (
    object_hash TEXT PRIMARY KEY,
    ref_count INTEGER NOT NULL DEFAULT 0,
    object_type TEXT NOT NULL, -- 'blob', 'tree', 'commit'
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

-- 引用关系表（用于追踪引用来源）
CREATE TABLE IF NOT EXISTS cas_ref_links (
    source_hash TEXT NOT NULL,
    target_hash TEXT NOT NULL,
    PRIMARY KEY (source_hash, target_hash),
    FOREIGN KEY (source_hash) REFERENCES cas_refs(object_hash),
    FOREIGN KEY (target_hash) REFERENCES cas_refs(object_hash)
);

-- CAS 引用表索引
CREATE INDEX IF NOT EXISTS idx_cas_refs_ref_count ON cas_refs(ref_count);
CREATE INDEX IF NOT EXISTS idx_cas_refs_object_type ON cas_refs(object_type);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_source ON cas_ref_links(source_hash);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_target ON cas_ref_links(target_hash);
```

### 9.2 修改表

```sql
-- chunks 表新增 CAS 对象引用
ALTER TABLE chunks ADD COLUMN cas_hash TEXT;

-- versions 表修改 storage_path 字段说明
-- storage_path 现在存储 CAS 提交哈希而不是文件路径
```

### 9.3 迁移脚本

```sql
-- 2026-06-02_cas_tables.sql

-- 1. 创建 CAS 相关表
CREATE TABLE IF NOT EXISTS cas_refs (
    object_hash TEXT PRIMARY KEY,
    ref_count INTEGER NOT NULL DEFAULT 0,
    object_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cas_ref_links (
    source_hash TEXT NOT NULL,
    target_hash TEXT NOT NULL,
    PRIMARY KEY (source_hash, target_hash),
    FOREIGN KEY (source_hash) REFERENCES cas_refs(object_hash),
    FOREIGN KEY (target_hash) REFERENCES cas_refs(object_hash)
);

-- 2. 创建索引
CREATE INDEX IF NOT EXISTS idx_cas_refs_ref_count ON cas_refs(ref_count);
CREATE INDEX IF NOT EXISTS idx_cas_refs_object_type ON cas_refs(object_type);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_source ON cas_ref_links(source_hash);
CREATE INDEX IF NOT EXISTS idx_cas_ref_links_target ON cas_ref_links(target_hash);

-- 3. chunks 表新增 cas_hash 字段
ALTER TABLE chunks ADD COLUMN cas_hash TEXT;

-- 4. 填充现有数据的 cas_hash（如果需要）
-- 注意：由于项目处于开发阶段，无需迁移现有数据
-- 新数据将自动使用 CAS 哈希

-- 5. versions 表说明
-- storage_path 字段现在存储 CAS 提交哈希而不是文件路径
-- 旧数据保持不变，新数据使用 CAS 哈希
```

---

## 十、实现计划

### 10.1 实现阶段

| 阶段     | 内容           | 预计工作量 |
| -------- | -------------- | ---------- |
| 阶段1    | CAS 核心层实现 | 3天        |
| 阶段2    | 领域层实现     | 2天        |
| 阶段3    | 版本管理重构   | 2天        |
| 阶段4    | AI体系适配     | 2天        |
| 阶段5    | 垃圾回收实现   | 1天        |
| 阶段6    | 并发控制实现   | 1天        |
| 阶段7    | 测试覆盖       | 3天        |
| 阶段8    | 文档更新       | 1天        |
| **总计** |                | **15天**   |

### 10.2 依赖关系

```
阶段1 (CAS核心层)
    │
    ├──阶段2 (领域层)
    │      │
    │      ├──阶段3 (版本管理)
    │      │      │
    │      │      └──阶段4 (AI适配)
    │      │
    │      └──阶段5 (垃圾回收)
    │
    └──阶段6 (并发控制)
           │
           └──阶段7 (测试覆盖)
                  │
                  └──阶段8 (文档更新)
```

---

## 十一、风险评估

### 11.1 技术风险

| 风险          | 影响 | 缓解措施             |
| ------------- | ---- | -------------------- |
| CAS 性能问题  | 高   | 基准测试、性能优化   |
| 数据一致性    | 高   | 事务管理、完整性校验 |
| AI 体系兼容性 | 中   | 适配层隔离、充分测试 |
| 存储空间增长  | 中   | 垃圾回收、压缩策略   |

### 11.2 进度风险

| 风险           | 影响 | 缓解措施     |
| -------------- | ---- | ------------ |
| 工作量估算不足 | 中   | 预留缓冲时间 |
| 依赖项延迟     | 低   | 提前识别依赖 |
| 测试覆盖不足   | 中   | TDD 开发模式 |

---

## 十二、成功标准

### 12.1 功能标准

- [ ] 文件创建、读取、更新、删除正常工作
- [ ] 版本快照创建、恢复、删除正常工作
- [ ] 回收站功能正常工作
- [ ] AI 补丁应用正常工作
- [ ] 垃圾回收正常工作
- [ ] 乐观锁正常工作

### 12.2 性能标准

- [ ] 文件读取延迟 < 10ms
- [ ] 版本创建延迟 < 50ms
- [ ] 版本恢复延迟 < 100ms
- [ ] 垃圾回收不影响正常使用

### 12.3 质量标准

- [ ] 测试覆盖率达标
- [ ] 无 P0/P1 级别 bug
- [ ] 文档完整
- [ ] 代码符合项目规范

---

_文档结束_
