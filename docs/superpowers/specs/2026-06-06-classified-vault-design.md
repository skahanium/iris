# 涉密保险库 (Classified Vault) 设计规格

_2026-06-06 · v1.0_

---

## 一、概述

为 Iris 引入文件夹级涉密文件系统。用户可将敏感笔记存放于唯一涉密文件夹 `.classified/` 中，文件在磁盘上透明加密（AES-256-GCM），仅在输入密码解锁后方可访问。涉密内容在任何状态下均不进入全文检索、语义搜索、向量索引或 AI 检索管道。同时新增编辑器锁定功能，允许用户将任意笔记标签页置为只读模式，状态持久化至数据库。

---

## 二、需求

### 2.1 功能需求

| 编号 | 需求                                                                       | 优先级 |
| ---- | -------------------------------------------------------------------------- | ------ |
| R1   | 首次使用时引导用户设置主密码，一个 vault 共用唯一密码                      | P0     |
| R2   | 密码通过 Argon2id 派生 256-bit AES 密钥，仅内存持有，永不落盘              | P0     |
| R3   | `.classified/` 目录内所有 `.md` 文件在写入时自动加密、读取时自动解密       | P0     |
| R4   | 加密文件格式：`CSEF` magic（4 字节）+ 12 字节随机 nonce + AES-256-GCM 密文 | P0     |
| R5   | 涉密面板通过 `Cmd+Shift+L` 快捷键唤起，主界面不作任何提示                  | P0     |
| R6   | 锁定状态下涉密面板仅显示密码输入框；解锁后展示 `.classified/` 内文件列表   | P0     |
| R7   | 涉密文件永远不出现在主文件树、搜索、AI 检索结果中                          | P0     |
| R8   | 涉密文件的历史版本与当前版本同等保护（CAS 层 `CASE` 加密）                 | P0     |
| R9   | 解锁后 10 分钟无操作自动锁定；若编辑器中有涉密文件打开，延后至文件关闭     | P0     |
| R10  | 编辑器右上角提供锁定按钮，将当前笔记标记为只读，状态持久化至 `files` 表    | P0     |
| R11  | 涉密面板内支持导入（普通 → 涉密）和导出（涉密 → 普通）转换操作             | P1     |
| R12  | 涉密面板内支持文件的创建、重命名、删除、子目录管理                         | P1     |
| R13  | 遗忘密码 = 永久丢失涉密数据，无后门恢复机制                                | —      |

### 2.2 非功能需求

| 编号 | 需求                                                                             |
| ---- | -------------------------------------------------------------------------------- |
| N1   | 加密/解密对编辑体验零感知延迟（AES-256-GCM 硬件加速，单次操作 < 1ms）            |
| N2   | 不从磁盘索引涉密文件的任何内容（FTS5、向量嵌入、语义锚点、法规索引、图谱链接）   |
| N3   | 密钥锁定后立即从内存零化，不留残留                                               |
| N4   | `.classified/` 被 `is_user_note_path()` 路径守卫统一过滤，复用现有安全边界       |
| N5   | 新增代码遵循现有模块组织规范（`crypto/`、`commands/`、`components/classified/`） |

### 2.3 范围排除

- 不实现多条密码、不实现密码找回
- 不实现多个涉密文件夹（永久限于 `.classified/`）
- 不实现涉密文件间的 wiki-link 索引（图谱中不可见）
- 不实现涉密文件的标签索引
- 不实现非当前 vault 的跨 vault 涉密共享

---

## 三、架构

### 3.1 总体结构

```
┌──────────────────────────────────────────────┐
│                  Iris 主界面                  │
│                                              │
│  ┌────────┐ (Cmd+Shift+E)     ┌───────────┐ │
│  │普通文件│ ← 悬浮面板         │ 编辑器区域 │ │
│  │树      │   不含 .classfied  │           │ │
│  └────────┘                   │ ┌───────┐ │ │
│                               │ │🔒锁定 │ │ │ ← 编辑器右上角
│                               │ └───────┘ │ │
│  搜索 / AI / 图谱 / 反向链接  │ 编辑器正文│ │
│  均不含涉密内容               │           │ │
│                               └───────────┘ │
│                                              │
│  ── 主界面无任何涉密功能入口 ──               │
└──────────────────────────────────────────────┘
              ↑ Cmd+Shift+L (无其他入口)
┌──────────────────────────────────────────────┐
│    涉密面板 (ClassifiedPanel, 独立悬浮层)     │
│                                              │
│  ┌ 锁定 ──────────────────────────────────┐ │
│  │  输入密码                    [确认]     │ │
│  └────────────────────────────────────────┘ │
│  ┌ 解锁 ──────────────────────────────────┐ │
│  │  .classified/                           │ │
│  │  ├── secret_plan.md                    │ │
│  │  ├── keys/                             │ │
│  │  │   └── tokens.md                     │ │
│  │  │                                      │ │
│  │  [新建] [导入]    [锁定]                │ │
│  └────────────────────────────────────────┘ │
└──────────────────────────────────────────────┘
```

### 3.2 模块依赖图

```
                    ┌───────────────┐
                    │  vault.json   │ (salt + verification_hash)
                    └───────┬───────┘
                            │
                    ┌───────▼───────┐
                    │  VaultKey     │ (Argon2id 派生/验证/持有)
                    └───────┬───────┘
                            │
              ┌─────────────┼─────────────┐
              │             │             │
     ┌────────▼───┐  ┌─────▼──────┐  ┌───▼──────────┐
     │classified  │  │  commands/ │  │  commands/   │
     │_io.rs      │  │  file.rs   │  │  classified  │
     │encrypt/dec │  │  读写管道  │  │  .rs IPCs    │
     └────────────┘  └────────────┘  └──────────────┘
                            │
              ┌─────────────┼─────────────┐
              │             │             │
     ┌────────▼───┐  ┌─────▼──────┐  ┌───▼──────────┐
     │ indexer/   │  │  watcher/  │  │  version/    │
     │ scan.rs    │  │  mod.rs    │  │  mod.rs      │
     │ (gate)     │  │  (skip)    │  │  (transparent)│
     └────────────┘  └────────────┘  └──────────────┘
```

---

## 四、详细设计

### 4.1 密码与密钥管理 (`crypto/vault_key.rs`)

#### 4.1.1 vault.json 格式

```json
{
  "version": 1,
  "salt": "a1b2c3...",
  "verification": "d4e5f6..."
}
```

- `salt`: 32 字节 CSPRNG 随机盐，hex 编码
- `verification`: 固定明文 `"iris-classified-vault-verify"` 经 AES-256-GCM 加密后的 hex 编码输出（nonce + ciphertext）

#### 4.1.2 密钥派生

```
Argon2id(
  password: &[u8],
  salt: &[u8; 32],
  config: Argon2id { memory: 19456 KiB, iterations: 2, parallelism: 1 }
) → [u8; 32]
```

参数选择依据：macOS 安全飞地（Secure Enclave）未直接暴露；选择适中参数平衡安全性与解锁延迟（目标 < 500ms）。

#### 4.1.3 生命周期

```rust
pub struct VaultKey {
    key: Option<Zeroizing<[u8; 32]>>,
}

impl VaultKey {
    /// 首次设密：生成随机 salt，派生密钥，加密验证文本，写入 vault.json
    pub fn setup(password: &str, vault_path: &Path) -> AppResult<()>;

    /// 解锁：读取 vault.json，派生密钥，解密验证文本比对
    pub fn unlock(password: &str, vault_path: &Path) -> AppResult<()>;

    /// 锁定：零化内存中密钥
    pub fn lock(&mut self);

    /// 当前是否已解锁
    pub fn is_unlocked(&self) -> bool;

    /// 获取密钥引用（调用方需先检查 is_unlocked）
    pub fn key(&self) -> AppResult<&[u8; 32]>;

    /// 检查 vault.json 是否存在（区分 NeedsSetup / NeedsUnlock）
    pub fn is_initialized(vault_path: &Path) -> bool;
}
```

全局状态：`static VAULT_KEY: OnceLock<RwLock<VaultKey>>`，在 `app.rs` 初始化时注入。

#### 4.1.4 错误处理

| 场景            | 错误类型                           | 用户消息               |
| --------------- | ---------------------------------- | ---------------------- |
| 密码错误        | `ClassifiedError::WrongPassword`   | "密码不正确"           |
| vault.json 损坏 | `ClassifiedError::ConfigCorrupted` | "保险库配置文件已损坏" |
| 未设密即解锁    | `ClassifiedError::NotInitialized`  | "请先设置保险库密码"   |
| 文件 I/O 失败   | 透传 `std::io::Error`              | 系统错误消息           |

---

### 4.2 文件加密 (`crypto/classified_io.rs`)

#### 4.2.1 磁盘格式

```
偏移  长度     内容
0      4       魔法 "CSEF" (0x43 0x53 0x45 0x46)
4      12      随机 nonce (每写新生成)
16     N       AES-256-GCM 密文（含 16 字节 auth tag）
```

```rust
const CSEF_MAGIC: &[u8; 4] = b"CSEF";
const NONCE_SIZE: usize = 12;

pub fn encrypt_cef(plaintext: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>>;
pub fn decrypt_cef(ciphertext: &[u8], key: &[u8; 32]) -> AppResult<Vec<u8>>;
pub fn has_csef_magic(data: &[u8]) -> bool;
```

#### 4.2.2 文件 I/O 管道改动

**`file_read`** (`commands/file.rs`):

返回值目前是 `AppResult<String>`。需改为结构体以承载 `is_locked` 字段：

```rust
#[derive(serde::Serialize)]
pub struct FileReadResult {
    pub content: String,
    pub is_locked: bool,
}

pub fn file_read(...) -> AppResult<FileReadResult> {
    let raw = std::fs::read(&full_path)?;
    let content = if has_csef_magic(&raw) {
        let key = VAULT_KEY.get().unwrap().read().unwrap();
        String::from_utf8(decrypt_cef(&raw, key.key()?)?)?
    } else {
        String::from_utf8(raw)?
    };
    let is_locked = db.query_is_locked(&path)?;
    Ok(FileReadResult { content, is_locked })
}
```

⚠️ 签名变更 → 需同步更新 `src/types/ipc.ts` 中的 `FileReadResult` 类型和 `src/lib/ipc.ts` 中的 `fileRead()` 封装函数。

**`file_write`** (`commands/file.rs`):

```rust
pub fn file_write(...) -> AppResult<()> {
    let data: Vec<u8> = if is_classified_path(&path) {
        let key = VAULT_KEY.get().unwrap().read().unwrap();
        encrypt_cef(content.as_bytes(), key.key()?)?
    } else {
        content.as_bytes().to_vec()
    };
    atomic_write(&full_path, &data)?;
    // ... re-index (skipped for classified files)
}
```

#### 4.2.3 边界情况

| 场景                        | 行为                                                                       |
| --------------------------- | -------------------------------------------------------------------------- |
| 加密文件被外部编辑器修改    | CSEF magic 损坏或 GCM tag 不匹配 → 解密失败 → 提示"文件已损坏或格式不正确" |
| 解锁前尝试读取涉密文件      | `file_read` 检测 CSEF → 调用 `key()` → 返回 `NotUnlocked` 错误             |
| 明文文件放入 `.classified/` | 无 CSEF magic → 按明文读取（兼容未加密迁移文件）→ 下次保存自动加密         |
| 大文件 (>20MB)              | 现有 `file_write` 已有 MAX_FILE_SIZE = 20MB 限制，不变                     |

---

### 4.3 搜索与检索排除

#### 4.3.1 路径守卫扩展

```rust
// storage/paths.rs
pub fn is_user_note_path(relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    !normalized.starts_with(".iris/")
        && !normalized.starts_with(".classified/")
        && normalized != ".iris"
        && normalized != ".classified"
}
```

`.classified/` 被此守卫排除后，自动获得与 `.iris/` 同等的全管道隔离：

| 管道                             | 受 `is_user_note_path` 保护？                         |
| -------------------------------- | ----------------------------------------------------- |
| `collect_vault_files()` 文件遍历 | ✅ (WalkDir filter_entry)                             |
| `index_file_with_embed()`        | ✅ (入口 check)                                       |
| `index_file_from_content()`      | ✅ (入口 check)                                       |
| `index_vault_incremental()`      | ✅ (入口 check)                                       |
| `file_list` SQL 查询             | ✅ (`path NOT LIKE '.iris/%'` + 新增 `.classified/%`) |
| watcher 文件变更                 | ✅ (入口 check)                                       |

#### 4.3.2 不涉及的模块（确认无操作）

以下模块的 SQL 查询均 JOIN `files` 表，而涉密文件永不进入 `files` 表：

- `ai_runtime/retrieval_broker.rs` — 5 层检索全部 JOIN `files`
- `ai_runtime/context_planner.rs` — 无直接查询
- `ai_runtime/prompt_builder.rs` — 无直接查询
- `knowledge/anchors.rs`, `regulations.rs`, `templates.rs` — JOIN `files`
- `embedding/engine.rs` — 通过 `chunks` → `files` 关联
- `indexer/wikilink.rs` — 目标文件需存在于 `files` 表

#### 4.3.3 已索引文件的清理

用户可在任何时间将 `.md` 文件移入 `.classified/`。导入操作（`classified_import`）需负责：

1. 在 `files` 表中删除该行（CASCADE 删除关联的 chunks、embeddings、links、tags）
2. 手动清理 FTS 索引：`DELETE FROM files_fts WHERE path = ?`
3. 若文件当前在编辑器中打开，通过 Tauri event `classified:file_taken` 通知前端关闭标签

---

### 4.4 编辑器锁定

#### 4.4.1 数据模型

```sql
-- migrations/023_file_lock.sql
ALTER TABLE files ADD COLUMN is_locked INTEGER NOT NULL DEFAULT 0;
```

`0` = 可编辑，`1` = 锁定（只读）。

#### 4.4.2 IPC

```rust
#[tauri::command]
fn file_set_lock(state: AppState, path: String, locked: bool) -> AppResult<()> {
    let conn = state.db()?;
    conn.execute(
        "UPDATE files SET is_locked = ?1 WHERE path = ?2",
        [locked as i64, &path]
    )?;
    Ok(())
}
```

`file_read` 返回值扩展 `is_locked: bool` 字段。

#### 4.4.3 前端行为

| 组件                              | 锁定状态行为                                                                             |
| --------------------------------- | ---------------------------------------------------------------------------------------- |
| `TipTapEditor.tsx`                | `useEditor({ editable: !locked })` — TipTap 自动禁用所有键盘输入和 history               |
| `DocumentTitleField.tsx`          | `readOnly={locked}`                                                                      |
| `useEditorContextMenu.ts`         | `handleContextMenu` 中 `locked` → 直接 return（禁止右键菜单）                            |
| `editor-actions.ts`               | `EditorActionContext` 新增 `isLocked: bool`，所有编辑类 action 的 `isEnabled` 检查此字段 |
| 斜杠命令 `/`                      | TipTap suggestion plugin 在 `editable=false` 时自动不触发                                |
| 撤销/重做 (`Cmd+Z`/`Cmd+Shift+Z`) | TipTap history 扩展在 `editable=false` 时禁用                                            |
| 选择、复制、搜索                  | 正常可用                                                                                 |
| 锁定按钮 UI                       | 编辑器右上角简约锁图标（SVG），locked 时显示闭合锁，unlocked 时显示打开锁                |

---

### 4.5 涉密面板 UI

#### 4.5.1 状态机

```
           首次唤起
               │
    ┌──────────▼──────────┐
    │   vault.json 存在?   │
    └──────┬─────────┬────┘
       是 │         │ 否
    ┌──────▼───┐  ┌──▼────────────┐
    │ NeedsUnlock│  │  NeedsSetup    │
    │ 密码输入框 │  │ 密码+确认+警告 │
    └──────┬───┘  └──────┬─────────┘
       验证成功│          │设置成功
    ┌──────▼───────────▼───┐
    │     Unlocked          │
    │  涉密文件树+操作按钮   │
    └──────┬────────────┬───┘
    手动锁定│            │10分钟空闲
    ┌──────▼───┐  ┌─────▼─────────┐
    │ 检查打开  │  │  WaitingLock   │
    │ 的涉密文件 │  │ (延后等待)     │
    └──┬────┬──┘  └───┬────────────┘
    有 │    │ 无      │文件关闭
    ┌──▼──┐┌─▼──┐   ┌─▼────────────┐
    │延后 ││立即│   │    Locked      │
    │等待 ││锁定│   │ 密钥零化，面板  │
    └──┬──┘└────┘   │ 回到密码输入框  │
       │            └────────────────┘
   文件关闭
       │
    ┌──▼────────────┐
    │ 还有涉密文件   │
    │ 打开中?        │
    └──┬─────────┬──┘
    是 │         │ 否
    ┌──▼──┐   ┌──▼──┐
    │继续 │   │锁定 │
    │等待 │   │     │
    └─────┘   └─────┘
```

#### 4.5.2 组件结构

```
ClassifiedPanel.tsx
├── ClassifiedPasswordSetup.tsx    (首次设密)
├── ClassifiedPasswordPrompt.tsx   (解锁)
└── ClassifiedFileList.tsx         (解锁后)
    ├── 文件树 (.classified/ 内目录结构)
    ├── 右键菜单: 打开 / 重命名 / 删除 / 导出
    ├── 底部工具栏: [新建] [导入] [锁定]
    └── 自动锁定倒计时 (10分钟)
```

#### 4.5.3 唤起方式

- `Cmd+Shift+L` 全局快捷键
- 涉密面板以 `IrisOverlay` 层叠显示（与命令面板、设置面板同级）
- `Escape` 关闭面板时自动锁定（如果已解锁）
- 主界面任何地方不出现 `Cmd+Shift+L` 的提示，也不显示涉密入口按钮或菜单项

---

### 4.6 导入 / 导出（分类 ↔ 普通）

#### 4.6.1 导入（普通 → 涉密）

```
ClassifiedFileList → [导入文件] 按钮
  → 弹出普通文件选择器（排除 .iris/ .classified/ 路径）
  → 选择目标文件
  → 后端 classified_import(path, ".classified/" 或子目录)
      → 验证: path 在用户笔记路径内 && 不在 .classified/ 内
      → 确保目标目录存在
      → fs::rename(path, target)
      → DELETE FROM files WHERE path = ?
      → DELETE FROM files_fts WHERE path = ?
      → (CASCADE 自动清理 chunks/embeddings/links)
      → 通知前端: 如该文件正在编辑, 强制关闭标签
      → 刷新涉密文件列表
```

#### 4.6.2 导出（涉密 → 普通）

```
ClassifiedFileList → 右键文件 → [导出]
  → 选择 vault 内目标目录
  → 后端 classified_export(path, target_folder)
      → 验证: VaultKey 已解锁
      → 读取加密文件 → 解密
      → 写入目标路径 (纯 UTF-8 明文, 无 CSEF magic)
      → fs::remove_file(原加密文件)
      → 触发目标路径 re-index
      → 刷新涉密文件列表
```

#### 4.6.3 冲突处理

目标路径存在同名文件时：

1. 前端检查 → 提示 "目标位置已存在 <filename>，是否覆盖？"
2. 用户确认 → 覆盖（先删除旧文件）
3. 用户拒绝 → 取消操作

---

### 4.7 版本快照

#### 4.7.1 兼容性保证

版本系统无需改动。数据流路径：

```
涉密文件编辑 → 保存
  → file_write: 明文 → 加密 → 写入 .classified/xxx.md (CSEF)
  → 版本快照触发
  → file_read: 读取 → 检测 CSEF → 解密 → 返回明文
  → CAS 存储: 明文 + CASE 加密 → .iris/cas/objects/
```

关键点：

- 版本系统通过 `file_read` 获取明文（解密透明）
- CAS 层 `CASE` 加密提供第二层保护（与普通文件版本相同）
- 版本恢复时同样通过 `file_write` 写回（加密透明）

---

## 五、数据流

### 5.1 涉密文件读取

```
用户打开涉密文件
  → ClassifiedFileList 选择文件
  → ipc.fileRead(path)
  → commands/file.rs: file_read
      → 检测 CSEF magic
      → VAULT_KEY.key() → 解密
      → 返回 UTF-8 明文
  → TipTapEditor.setContent(html)
  → 正常渲染
```

### 5.2 涉密文件写入

```
编辑器 save (Cmd+S / 自动保存)
  → serializeOpenNote() → markdown 字符串
  → ipc.fileWrite(path, content)
  → commands/file.rs: file_write
      → is_classified_path(path)?
        → 是: encrypt_cef(content, key)
        → 否: content.as_bytes()
      → atomic_write → .tmp → rename
  → 跳过 re-index (is_classified_path)
```

### 5.3 解锁流程

```
Cmd+Shift+L → ClassifiedPanel 唤起
  → 读取 init_state: is_initialized?
    → 否: NeedsSetup 界面
    → 是: NeedsUnlock 界面
  → 用户输入密码
  → classified_unlock(password) IPC
  → VaultKey::unlock()
    → Argon2id(password, salt) → key
    → decrypt_cef(verification, key) → 比对
    → 成功: key 写入 VaultKey.key
    → 失败: WrongPassword
  → 前端根据结果渲染 Unlocked 或 错误提示
```

### 5.4 自动锁定

```
Unlocked 状态 → 启动 idle timer (10分钟)
  → 空闲到期 → 进入 WaitingLock
  → 检查: 编辑器中有 classfied path 的打开标签?
    → 有: 保持 WaitingLock, 监听标签关闭事件
    → 无: 执行 lock
  → VaultKey::lock() → key.zeroize()
  → 前端面板切换到 Locked
```

---

## 六、IPC 接口一览

### 6.1 新增 IPC

| 命令                | 参数                                  | 返回                                                   | 前置条件                 |
| ------------------- | ------------------------------------- | ------------------------------------------------------ | ------------------------ |
| `classified_setup`  | `password: String`                    | `()`                                                   | vault.json 不存在        |
| `classified_unlock` | `password: String`                    | `()`                                                   | vault.json 存在 + 未解锁 |
| `classified_lock`   | —                                     | `()`                                                   | 已解锁                   |
| `classified_status` | —                                     | `"needs_setup" \| "locked" \| "unlocked" \| "waiting"` | —                        |
| `classified_files`  | `folder: Option<String>`              | `Vec<ClassifiedFileEntry>`                             | 已解锁                   |
| `classified_import` | `path: String, target_folder: String` | `()`                                                   | 已解锁                   |
| `classified_export` | `path: String, target_folder: String` | `()`                                                   | 已解锁                   |
| `file_set_lock`     | `path: String, locked: bool`          | `()`                                                   | path 是用户笔记路径      |

### 6.2 扩展现有 IPC

| 命令         | 改动                                                                                            |
| ------------ | ----------------------------------------------------------------------------------------------- |
| `file_read`  | 返回结构新增 `is_locked: bool`；对涉密文件自动解密                                              |
| `file_write` | 对涉密文件自动加密                                                                              |
| `file_list`  | SQL 过滤新增 `AND path NOT LIKE '.classified/%'` 排除项（安全网，主防线在 `is_user_note_path`） |

---

## 七、错误处理矩阵

| 场景                                  | 错误类型           | 用户感知                                                     |
| ------------------------------------- | ------------------ | ------------------------------------------------------------ |
| 解锁时密码错误                        | `WrongPassword`    | 密码框抖动 + 红色提示 "密码不正确"                           |
| 解锁时 vault.json 损坏                | `ConfigCorrupted`  | "保险库配置文件已损坏，请检查 .iris/vault.json"              |
| 编辑涉密文件时锁定触发                | 延后               | 无感知（文件关闭后才锁定）                                   |
| 锁定后尝试打开涉密文件                | `NotUnlocked`      | 不显示在文件列表中（无此场景）                               |
| 导入时目标已存在                      | `Conflict`         | 弹窗 "目标位置已存在同名文件，是否覆盖？"                    |
| 导出时涉密文件解密失败                | `DecryptionFailed` | "文件解密失败，文件可能已损坏"                               |
| 编辑器锁定状态下编辑                  | —                  | 无感知（键盘输入被 TipTap 自动忽略，右键菜单不弹出）         |
| 移动涉密文件至非涉密区（手动绕过 UI） | —                  | 下次读取时检测到 CSEF 但 KeyHolder 为空 → `NotUnlocked` 错误 |

---

## 八、测试策略

### 8.1 单元测试 (Rust)

| 模块                      | 测试点                                                                                            |
| ------------------------- | ------------------------------------------------------------------------------------------------- |
| `crypto/vault_key.rs`     | Argon2id 派生确定性 (同密码+同salt → 同密钥)；验证成功/失败；lock 后 key 不可获取；零化后内存清零 |
| `crypto/classified_io.rs` | 加密→解密往返；错误密钥解密失败；CSEF magic 检测；空文件；非 CSEF 数据；损坏密文                  |
| `storage/paths.rs`        | `.classified/` 及其子路径均被 `is_user_note_path` 返回 `false`                                    |
| `commands/classified.rs`  | setup→unlock→lock 生命周期；重复 setup 被拒；未 setup 即 unlock 报错；import/export 路径验证      |
| `commands/file.rs`        | 涉密文件 read/write 自动加密解密；locked 文件 read 返回 is_locked=true                            |

### 8.2 集成测试

| 场景          | 验证                                                   |
| ------------- | ------------------------------------------------------ |
| 全 vault 扫描 | `index_vault_incremental` 不返回 `.classified/` 内文件 |
| 文件监视      | `.classified/` 内文件变更不触发 re-index               |
| FTS 搜索      | 搜索涉密文件内的关键词不返回结果                       |
| 向量搜索      | `semantic_search` 不返回涉密文件 chunks                |
| AI 检索       | `hybrid_retrieve` 不返回涉密文件内容                   |
| 版本快照      | 涉密文件创建版本 → 恢复版本 → 内容一致且保持加密       |
| 导入导出往返  | 普通文件 → import → export → 内容一致、无 CSEF magic   |

### 8.3 前端测试

| 场景                   | 验证                                                  |
| ---------------------- | ----------------------------------------------------- |
| ClassifiedPanel 状态机 | Setup → Locked → Unlocked → Waiting → Locked 状态转换 |
| 密码错误               | 显示错误提示、输入框清空                              |
| 编辑器锁定             | 按钮切换后 editable 变更；右键菜单禁止；标题只读      |
| 快捷键                 | `Cmd+Shift+L` 唤起面板、`Escape` 关闭并锁定           |

---

## 九、迁移

### 9.1 数据库

`migrations/023_file_lock.sql`:

```sql
ALTER TABLE files ADD COLUMN is_locked INTEGER NOT NULL DEFAULT 0;
```

增量 migration，不影响现有数据。默认 `0` 确保所有现有文件处于可编辑状态。

### 9.2 文件系统

- 首次设密时自动创建 `.classified/` 空目录（位于 vault 根目录）
- `.iris/vault.json` 新增（首次设密时），含 `salt` 和 `verification`
- 现有 `.md` 文件不受影响，仅当用户主动导入后才进入加密状态

### 9.3 回滚

- 删除 `.iris/vault.json` → 下次唤起涉密面板时重新走 NeedsSetup 流程（⚠️ 原涉密数据将无法解密）
- `023_file_lock.sql` 对应的 down 脚本：

```sql
-- 023_file_lock_down.sql
CREATE TABLE files_new AS SELECT id, path, title, frontmatter, content_hash, word_count, genre, created_at, updated_at FROM files;
DROP TABLE files;
ALTER TABLE files_new RENAME TO files;
```

---

## 十、风险评估与缓解

| 风险                         | 影响                 | 缓解措施                                                                          |
| ---------------------------- | -------------------- | --------------------------------------------------------------------------------- |
| 密码遗忘                     | 涉密数据永久不可恢复 | 首次设密时醒目警告；不接受无密码强度要求                                          |
| vault.json 丢失/损坏         | 涉密数据永久不可恢复 | 在 `classified_setup` 时警告此文件不可删除                                        |
| 外部编辑器破坏 CSEF 格式     | 单文件不可解密       | 限定涉密文件仅通过 Iris 编辑；解密失败提供明确诊断信息                            |
| Argon2 性能                  | 解锁延迟 > 1s        | 参数设定 (< 500ms)，实测后调整                                                    |
| 涉密文件在内存中明文残留     | 内存转储泄露         | 编辑器生命周期内明文不可避免；锁定后 Zeroizing 清理密钥；未来可评估 mlock/madvise |
| 锁定延后导致长时间未实际锁定 | 安全窗口期过长       | 10 分钟自动锁定；面板内显示倒计时                                                 |

---

## 十一、未来扩展（明确不在现阶段实现）

- 涉密文件内 wiki-link 解析与涉密内图谱
- 多密码 / 共享涉密区
- 硬件安全模块 (HSM) / Secure Enclave 密钥保护
- 密码强度策略
- 生物认证解锁 (Touch ID / Windows Hello)
- 涉密文件远程同步冲突处理

---

_本文档为涉密保险库功能的权威设计来源。任何实现偏差必须在 PR 中说明理由。_
