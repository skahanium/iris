# Iris RSS Subscription Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不接入 Agent/RAG/MCP、也不自动改写用户 Vault 的前提下，为 Iris 交付可离线、可迁移、可搜索的 RSS 订阅资料库。

**Architecture:** 订阅源、文章 Markdown、转换前载荷和阅读状态存入应用级 SQLite；Rust 负责安全获取、Feed 解析、HTML → Markdown、增量同步和 FTS，React 只接收净化后的 DTO。实现只增加两张事实表和一个 `feed` 模块；同步复用现有 Scheduler，不建设通用 provider 或任务系统。收件箱是查询，显式「保存为笔记」复用现有文档持久化协调器。

**Tech Stack:** Tauri 2.x、Rust 1.85、tokio、rusqlite/SQLite FTS5、reqwest/rustls、feed-rs、htmd、React 19、TypeScript、TailwindCSS + shadcn/ui、marked、DOMPurify、Vitest、Cargo test。

## Global Constraints

- 遵守根目录 `AGENTS.md`，测试先行，禁止新增 `unsafe`。
- 这是单人项目：优先复用现有模块，不为未来需求预建通用抽象。
- 支持和验收平台限定为 macOS、Windows。
- 新增依赖必须与 AGPL-3.0 兼容并在变更说明中记录理由。
- IPC 同步更新 Rust、`src/types/ipc.ts`、`src/lib/ipc.ts`、测试和文档。
- migration 必须有 down；不创建 worktree；除明确「保存为笔记」外不触碰用户 `.md`。
- 产品与数据契约以 [RSS 订阅资料库规范](../../rss-subscription-library.md) 为准。

---

## 实施总览与依赖顺序

```text
阶段 0 契约与依赖
  → 阶段 1 Schema/Repository
  → 阶段 2 安全获取/转换/同步
  → 阶段 3 IPC
  → 阶段 4 React 阅读工作区
  → 阶段 5 OPML/保存为笔记/发布
```

阶段 1–3 是后端纵向闭环，不能并行修改同一 migration/command 注册。阶段 4 可在 IPC DTO 冻结后开始。每个阶段独立提交、独立回归；任何硬门禁失败都留在当前阶段修复，不把风险推到发布阶段。

## 阶段 0：契约冻结、fixture 与依赖门禁

### Task 0.1：冻结结构性产品决定

**Files:**

- Modify: `docs/rss-subscription-library.md`
- Modify: `ROADMAP.md`

- [x] 与 maintainer 逐项确认：跨 Vault 共享、历史默认已读、远程图片默认阻止、HTTPS-only、不自动清理、保存为独立笔记。
- [x] 特别确认「跨 Vault 共享」；若改变，先修订规范和本文所有 `AppState.db` 假设，再开始 migration。
- [x] 在规格文档头部把状态从「规划基线」改为「已冻结」，记录日期，不写版本号承诺。
- [x] 运行 `npm run docs:check`，预期 exit 0。
- [x] 提交：`docs(rss): 冻结订阅资料库产品契约`。

### Task 0.2：建立格式、安全与更新 fixture

**Files:**

- Create: `src-tauri/tests/fixtures/feeds/rss2-basic.xml`
- Create: `src-tauri/tests/fixtures/feeds/atom-xhtml.xml`
- Create: `src-tauri/tests/fixtures/feeds/rss1-rdf.xml`
- Create: `src-tauri/tests/fixtures/feeds/json-feed.json`
- Create: `src-tauri/tests/fixtures/feeds/duplicate-guid.xml`
- Create: `src-tauri/tests/fixtures/feeds/item-update-v1.xml`
- Create: `src-tauri/tests/fixtures/feeds/item-update-v2.xml`
- Create: `src-tauri/tests/fixtures/feeds/malformed.xml`
- Create: `src-tauri/tests/fixtures/feeds/xxe.xml`
- Create: `src-tauri/tests/fixtures/feeds/unsafe-html.xml`
- Create: `src-tauri/tests/fixtures/feeds/relative-links.xml`
- Create: `src-tauri/tests/fixtures/opml/nested.opml`
- Create: `src-tauri/tests/fixtures/opml/duplicate-urls.opml`
- Create: `src-tauri/tests/fixtures/opml/oversized-title.opml`
- Modify: `docs/testing/rss-subscription-library-manual-checklist.md`

- [x] 每个 Feed fixture 固定 2–5 个条目，使用 `example.com`，不得含真实用户订阅或版权正文。
- [x] `item-update-v2.xml` 保持 GUID，只改变标题、正文、`updated`，用于证明阅读状态不被更新覆盖。
- [x] `unsafe-html.xml` 覆盖 script、style、iframe、表单、事件属性、`javascript:`、相对链接和远程图片。
- [x] `xxe.xml` 同时覆盖 `DOCTYPE` 与 `ENTITY`，期望在 parser 前以稳定码 `feed_xml_unsafe_declaration` 拒绝。
- [x] 手工清单先写成未执行状态，覆盖 `800/1024/1280/1366/1440/1920 × 600/800/1080`、亮暗主题、200% 缩放、键盘、读屏、reduced motion、离线、代理、升级与回滚。
- [x] 运行 `rg -n "微信|mp.weixin|真实" src-tauri/tests/fixtures/feeds src-tauri/tests/fixtures/opml`，确认没有真实内容。
- [x] 提交：`test(rss): 添加订阅格式与安全基线语料`。

### Task 0.3：加入最小解析依赖

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

先加入精确版本，避免预 1.0 转换行为漂移：

```toml
feed-rs = { version = "=2.4.0", features = ["sanitize"] }
htmd = "=0.5.5"
```

- [x] 在变更说明中记录：`feed-rs` 避免自写多格式解析，`htmd` 避免自写 HTML → Markdown；两者分别为 MIT、Apache-2.0。
- [x] 添加两个精确依赖，不再加入第三个 sanitizer crate；使用 `feed-rs` 自带 sanitize feature 和前端现有 DOMPurify。
- [x] 运行 `cargo tree --manifest-path src-tauri/Cargo.toml -i feed-rs` 与 `cargo tree --manifest-path src-tauri/Cargo.toml -i htmd`，确认没有不兼容许可证。
- [x] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期 Rust 1.85 构建成功。
- [x] 运行 `npm run audit:rust`，预期无未登记高危漏洞。
- [x] 提交：`chore(rss): 引入受审查的 Feed 解析与转换依赖`。

**阶段 0 退出条件：** 六项产品决策冻结；fixture 可读；两个 crate 的许可、Rust 1.85 构建和安全审计通过。否则不得创建 `063` migration。

## 阶段 1：应用级资料库与 Repository

### Task 1.1：创建可回滚 Schema

**Files:**

- Create: `src-tauri/migrations/063_feed_library.sql`
- Create: `src-tauri/migrations/063_feed_library.down.sql`
- Modify: `src-tauri/src/storage/migrate.rs`

- [x] 先在 `migrate.rs` 测试模块添加 `migration_063_creates_feed_library_and_fts`、`migration_063_roundtrip`、`migration_063_is_idempotent`。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml storage::migrate::tests::migration_063 -- --nocapture`，预期 RED：migration 未注册。
- [x] `063_feed_library.sql` 完整创建以下两个事实表：

```sql
CREATE TABLE feed_sources (
    id TEXT PRIMARY KEY,
    feed_url TEXT NOT NULL UNIQUE,
    site_url TEXT,
    title TEXT NOT NULL,
    title_override TEXT,
    description TEXT,
    icon_url TEXT,
    language TEXT,
    folder_path TEXT NOT NULL DEFAULT '',
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    fetch_interval_minutes INTEGER NOT NULL DEFAULT 60
        CHECK (fetch_interval_minutes BETWEEN 15 AND 10080),
    etag TEXT,
    last_modified TEXT,
    last_checked_at TEXT,
    last_success_at TEXT,
    next_fetch_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    last_error_code TEXT,
    last_error_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE feed_items (
    row_id INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    source_id TEXT NOT NULL REFERENCES feed_sources(id) ON DELETE CASCADE,
    external_key TEXT NOT NULL,
    canonical_url TEXT,
    title TEXT NOT NULL,
    author_name TEXT,
    published_at TEXT,
    source_updated_at TEXT,
    received_at TEXT NOT NULL,
    summary_markdown TEXT NOT NULL DEFAULT '',
    content_markdown TEXT NOT NULL,
    content_text TEXT NOT NULL,
    source_payload TEXT NOT NULL,
    source_payload_kind TEXT NOT NULL
        CHECK (source_payload_kind IN ('html', 'xhtml', 'text', 'markdown')),
    content_hash TEXT NOT NULL,
    conversion_version INTEGER NOT NULL,
    conversion_status TEXT NOT NULL
        CHECK (conversion_status IN ('ok', 'degraded')),
    read_at TEXT,
    starred_at TEXT,
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (source_id, external_key)
);
```

- [x] 在同一 migration 完整加入以下索引与 FTS trigger：

```sql
CREATE INDEX idx_feed_sources_due
    ON feed_sources(is_enabled, next_fetch_at);
CREATE INDEX idx_feed_sources_folder
    ON feed_sources(folder_path, title);
CREATE INDEX idx_feed_items_inbox
    ON feed_items(archived_at, read_at, received_at DESC, row_id DESC);
CREATE INDEX idx_feed_items_source_time
    ON feed_items(source_id, received_at DESC, row_id DESC);
CREATE INDEX idx_feed_items_starred
    ON feed_items(starred_at DESC) WHERE starred_at IS NOT NULL;
CREATE INDEX idx_feed_items_archived
    ON feed_items(archived_at DESC) WHERE archived_at IS NOT NULL;

CREATE VIRTUAL TABLE feed_items_fts USING fts5(
    title,
    author_name,
    content_text,
    content='feed_items',
    content_rowid='row_id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER feed_items_fts_ai AFTER INSERT ON feed_items BEGIN
    INSERT INTO feed_items_fts(rowid, title, author_name, content_text)
    VALUES (new.row_id, new.title, COALESCE(new.author_name, ''), new.content_text);
END;
CREATE TRIGGER feed_items_fts_ad AFTER DELETE ON feed_items BEGIN
    INSERT INTO feed_items_fts(feed_items_fts, rowid, title, author_name, content_text)
    VALUES ('delete', old.row_id, old.title, COALESCE(old.author_name, ''), old.content_text);
END;
CREATE TRIGGER feed_items_fts_au AFTER UPDATE OF title, author_name, content_text ON feed_items BEGIN
    INSERT INTO feed_items_fts(feed_items_fts, rowid, title, author_name, content_text)
    VALUES ('delete', old.row_id, old.title, COALESCE(old.author_name, ''), old.content_text);
    INSERT INTO feed_items_fts(rowid, title, author_name, content_text)
    VALUES (new.row_id, new.title, COALESCE(new.author_name, ''), new.content_text);
END;
```

- [x] down 脚本按 trigger → FTS → `feed_items` → `feed_sources` 顺序删除，不修改其他表。
- [x] 在 `migrate.rs` 增加 `MIGRATION_063_UP/DOWN`，up 末尾注册，down 开头回滚。
- [x] 运行 migration 三项测试，预期 GREEN。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml storage::migrate::tests::migration_registry_covers_all_sql_files`，预期 GREEN。
- [x] 提交：`feat(storage): 添加订阅资料库迁移`。

### Task 1.2：定义领域类型与 Repository 契约

**Files:**

- Create: `src-tauri/src/feed/mod.rs`
- Create: `src-tauri/src/feed/model.rs`
- Create: `src-tauri/src/feed/repository.rs`
- Create: `src-tauri/src/feed/repository_tests.rs`
- Modify: `src-tauri/src/lib.rs`

在 `model.rs` 定义并对 IPC 使用 camelCase：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSourceSummary {
    pub id: String,
    pub title: String,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub folder_path: String,
    pub is_enabled: bool,
    pub unread_count: i64,
    pub last_checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub next_fetch_at: Option<String>,
    pub consecutive_failures: i64,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedView { Inbox, Today, All, Starred, Archived }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedPageCursor { pub sort_at: String, pub row_id: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemQuery {
    pub view: FeedView,
    pub source_id: Option<String>,
    pub received_after: Option<String>,
    pub cursor: Option<FeedPageCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemSummary {
    pub id: String,
    pub source_id: String,
    pub source_title: String,
    pub title: String,
    pub author_name: Option<String>,
    pub canonical_url: Option<String>,
    pub published_at: Option<String>,
    pub received_at: String,
    pub excerpt: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub is_archived: bool,
    pub conversion_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedItemDetail {
    pub summary: FeedItemSummary,
    pub content_markdown: String,
    pub summary_markdown: String,
}
```

- [x] 先写 repository RED 测试：source CRUD、收件箱派生、三个状态轴独立、cursor 稳定、source cascade、FTS 更新、详情 DTO 无 `source_payload`。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml feed::repository_tests -- --nocapture`，预期 RED。
- [x] 实现 `FeedRepository`，所有批量 upsert 使用一个 SQLite 事务；`limit` clamp 到 `1..=200`。
- [x] 列表摘要从 `content_text` 截断到 240 Unicode scalar，不切坏 UTF-8；详情只返回规范化 Markdown。
- [x] FTS 查询转义用户输入，不拼接原始 SQL；空查询返回验证错误。
- [x] item 更新只在 `content_hash` 改变时替换内容字段，绝不覆盖三个状态时间戳和 `received_at`。
- [x] 运行 repository 测试，预期 GREEN。
- [x] 运行 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`。
- [x] 提交：`feat(storage): 实现订阅资料库仓储层`。

**阶段 1 退出条件：** migration up/down/idempotent；CRUD/FTS/状态机全部在内存 SQLite 通过；无网络、无 UI、无 Vault 写入。

## 阶段 2：安全获取、解析、Markdown 与同步

### Task 2.1：抽取可复用的公共 HTTPS 出站校验

**Files:**

- Create: `src-tauri/src/network/safe_https.rs`
- Modify: `src-tauri/src/network/mod.rs`
- Modify: `src-tauri/src/llm/fetch_web_page.rs`
- Create: `src-tauri/src/network/safe_https_tests.rs`

- [ ] 先把现有 `fetch_web_page.rs` 对 localhost、IPv4/IPv6 私网、metadata、重绑定提示的测试复制为公共模块契约；加入 userinfo、重定向再校验、混合公共/私网 DNS 测试。
- [ ] 运行公共模块测试，预期 RED。
- [ ] 将纯校验和 DNS pinning 抽到 `network::safe_https`，保留网页抓取原行为；不得复制两套地址判断。
- [ ] 公共 API 仅暴露有界能力：`validate_https_url`、`resolve_public_addrs`、`build_pinned_client`、`validate_redirect_target`。
- [ ] reqwest redirect policy 固定为 none，由调用方逐跳处理；任何一条解析地址被拒绝都拒绝该主机。
- [ ] 运行原 `fetch_web_page` 全部测试和新测试，预期 GREEN。
- [ ] 提交：`refactor(network): 统一公开 HTTPS 地址校验`。

### Task 2.2：实现有界 Feed/发现页获取

**Files:**

- Create: `src-tauri/src/feed/fetch.rs`
- Create: `src-tauri/src/feed/fetch_tests.rs`
- Modify: `src-tauri/src/feed/mod.rs`

固定配置：

```rust
pub(crate) const FEED_MAX_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const DISCOVERY_MAX_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const MAX_REDIRECTS: usize = 5;
```

- [ ] RED 测试覆盖：200、有界 streaming、Content-Length 预拒绝、流中超限、304、ETag/Last-Modified、5 跳、循环重定向、重定向到私网、非 HTTPS、超时、系统代理策略。
- [ ] 实现 `FeedHttpClient::fetch(url, validators, purpose)`；返回 status、最终安全 URL、content-type、etag、last-modified、有界 bytes。
- [ ] User-Agent 只包含 `Iris/<version> RSS Reader`，不含 Vault、设备名或用户 ID。
- [ ] 日志只记录 source ID/状态类别/字节数/耗时；测试用 tracing capture 证明不含 URL 和 body fixture。
- [ ] 运行 `cargo test ... feed::fetch_tests`，预期 GREEN。
- [ ] 提交：`feat(rss): 实现安全有界的订阅获取`。

### Task 2.3：解析与规范化

**Files:**

- Create: `src-tauri/src/feed/normalize.rs`
- Create: `src-tauri/src/feed/normalize_tests.rs`
- Modify: `src-tauri/src/feed/mod.rs`

```rust
pub(crate) const FEED_CONVERSION_VERSION: i64 = 1;

pub(crate) struct NormalizedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub items: Vec<NormalizedItem>,
}

pub(crate) struct NormalizedItem {
    pub external_key: String,
    pub canonical_url: Option<String>,
    pub title: String,
    pub author_name: Option<String>,
    pub published_at: Option<String>,
    pub source_updated_at: Option<String>,
    pub summary_markdown: String,
    pub content_markdown: String,
    pub content_text: String,
    pub source_payload: String,
    pub source_payload_kind: SourcePayloadKind,
    pub content_hash: String,
    pub conversion_status: ConversionStatus,
}
```

- [ ] 用阶段 0 全部 fixture 写 RED table-driven tests，断言格式、稳定键、绝对链接、危险节点、远程图片标记、UTF-8、末尾换行和 degraded 回退。
- [ ] parser 前对 ASCII case-insensitive 的 `<!DOCTYPE` / `<!ENTITY` 拒绝，错误码固定。
- [ ] 使用 `feed-rs` sanitize feature 解析；HTML/XHTML 再经 `htmd`，skip `script/style/iframe/form/svg/math`。
- [ ] 相对链接只以安全的文章 HTTPS URL 为 base；不安全 URL 转纯文本。
- [ ] `content_text` 从最终 Markdown 确定性去标记生成，不使用浏览器 DOM。
- [ ] 标题上限 500 Unicode scalar，正文 Markdown 上限 4 MiB；超限以稳定规则截断并标 degraded。
- [ ] 运行 normalize tests，预期 GREEN；再运行 `cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] 提交：`feat(rss): 将订阅内容规范化为安全 Markdown`。

### Task 2.4：Feed 自动发现

**Files:**

- Create: `src-tauri/src/feed/discovery.rs`
- Create: `src-tauri/src/feed/discovery_tests.rs`

- [ ] RED 测试覆盖直接 Feed、HTML alternate、相对 href、重复候选、多候选排序、跨协议/私网拒绝、无候选。
- [ ] `discover(url)` 先尝试 bounded Feed parse；若 content-type/parse 表明 HTML，再只解析 `link[rel~=alternate]` 且 type 为 RSS/Atom/JSON Feed。
- [ ] 候选去重后最多返回 10 个；同源 host 优先，但多候选不自动订阅。
- [ ] DTO 不返回 HTML；仅返回安全 URL、候选标题和格式。
- [ ] 运行 discovery tests，预期 GREEN。
- [ ] 提交：`feat(rss): 添加订阅源自动发现`。

### Task 2.5：实现单源同步事务

**Files:**

- Create: `src-tauri/src/feed/sync.rs`
- Create: `src-tauri/src/feed/sync_tests.rs`
- Modify: `src-tauri/src/feed/mod.rs`

- [ ] RED 测试：首次历史默认已读/可选未读、后续新条目未读、304、内容更新保状态、重复 GUID、事务回滚、稳定错误码、退避时间。
- [ ] `sync_source(source_id, mode)` 先读取配置，获取/解析在 SQLite 连接外执行，最后用短事务 upsert。
- [ ] 首次同步在同一事务判断 source 尚无 item；默认给历史项目写 `read_at=received_at`。
- [ ] 成功清零 failures，保存 validators 和 `next_fetch_at`；304 同样视为成功。
- [ ] 失败只更新 `last_checked_at/last_error_code/last_error_at/consecutive_failures/next_fetch_at`，保留旧 validators 与文章。
- [ ] 退避固定为 15m/1h/6h/24h，不加入随机抖动。
- [ ] 运行 sync tests，预期 GREEN。
- [ ] 提交：`feat(rss): 完成订阅源增量同步事务`。

### Task 2.6：复用现有 Scheduler 做自动同步

**Files:**

- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/scheduler.rs`
- Modify: `src-tauri/src/feed/sync.rs`
- Modify: `src-tauri/src/feed/sync_tests.rs`

- [ ] RED 测试：同一 source 不能重复同步、暂停源跳过、到期查询只返回 2 个批次、失败后互斥标记释放。
- [ ] `AppState` 只增加一个 `FeedSyncService`；服务内部用 `tokio::sync::Mutex<HashSet<String>>` 防止同源重复，不创建 job 表或通用任务状态机。
- [ ] 在现有 `Scheduler` 增加 15 分钟 tick，每轮从 repository 取最多 2 个到期源并并发同步；不新增第二套 scheduler 文件。
- [ ] 手动刷新与自动刷新调用同一个 `sync_source`；应用重启依靠数据库中的 `next_fetch_at` 恢复。
- [ ] 运行 feed sync tests 和现有 scheduler tests，预期 GREEN。
- [ ] 提交：`feat(rss): 复用现有调度器同步订阅`。

**阶段 2 退出条件：** 全部格式 fixture 正确；SSRF、XXE、重定向、超限、危险 HTML、304、更新保状态和自动到期同步测试通过。

## 阶段 3：冻结 IPC 契约

### Task 3.1：增加类型安全命令

**Files:**

- Create: `src-tauri/src/commands/feed_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/types/ipc.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ipc-events.ts`
- Modify: `docs/ipc-api-reference.md`
- Create: `tests/feed-ipc-contract.test.ts`

TypeScript 契约必须与 Rust camelCase DTO 一致：

```ts
export type FeedView = "inbox" | "today" | "all" | "starred" | "archived";

export interface FeedItemStatePatch {
  isRead?: boolean;
  isStarred?: boolean;
  isArchived?: boolean;
}

export interface FeedChangedEvent {
  sourceId: string;
  kind: "sync_succeeded" | "sync_failed" | "items_changed";
  newItems: number;
  errorCode: string | null;
}
```

- [ ] 先写 IPC contract RED 测试，mock `invoke` 并固定命令名和 camelCase 参数；特别断言详情类型无 `sourcePayload`。
- [ ] 注册规范 §12 的全部命令；命令只做验证/授权边界和 service 调用，不内嵌 SQL。
- [ ] `feed_item_set_state` 要求 patch 至少一个字段；所有 ID/URL/string 长度有界。
- [ ] `feed_items_mark_read` 接收冻结 `FeedItemQuery`，返回影响行数。
- [ ] `feed_sync_source` 等待单源完成并返回计数；`feed_sync_all` 复用同一 service、最多 2 个并发。事件只提示 UI 重新查询，不建立 job 恢复协议。
- [ ] 更新 IPC 文档，明确 raw payload 永不出 IPC、同步事件不含 URL/正文。
- [ ] 运行 `npm run test -- feed-ipc-contract`、`npm run typecheck` 和 Rust command tests。
- [ ] 提交：`feat(ipc): 暴露类型安全的订阅资料库契约`。

**阶段 3 退出条件：** 可通过 IPC 完成发现→订阅→同步→列表→详情→状态→搜索；Rust、TypeScript、事件与 IPC 文档一致。

## 阶段 4：React 订阅工作区

### Task 4.1：建立前端状态层

**Files:**

- Create: `src/hooks/useFeedLibrary.ts`
- Create: `src/lib/feed-reader.ts`
- Create: `tests/use-feed-library.test.tsx`
- Create: `tests/feed-reader.test.ts`

- [ ] RED 测试：初始 inbox、source/view 切换、迟到响应丢弃、同步事件刷新、状态失败回滚、今日边界。
- [ ] Hook 保存 `view/sourceId/search/selectedItemId/page/status`，不把文章正文写 localStorage。
- [ ] 每次筛选变化递增 request epoch；迟到响应不得覆盖新视图。
- [ ] `feed-reader.ts` 只负责 Markdown 渲染配置、DOMPurify allowlist、外链拦截和远程图片占位。
- [ ] DOMPurify allowlist 禁止 style/iframe/form/video/audio/object/embed，链接只允许 HTTPS，所有外链点击调用 `openExternalHttpsUrl`。
- [ ] 运行两个测试文件，预期 GREEN。
- [ ] 提交：`feat(rss): 添加订阅前端状态与安全渲染层`。

### Task 4.2：增加应用工作区模式而不卸载编辑器

**Files:**

- Modify: `src/App.impl.tsx`
- Modify: `src/components/layout/AppShell.tsx`
- Modify: `src/components/layout/DesktopTitleBar.tsx`
- Modify: `src/hooks/useWorkspaceChromeActions.tsx`
- Create: `tests/app-shell-feed-workspace.test.tsx`
- Modify: `tests/app-shell-adaptive-layout.test.tsx`

引入独立模式，不扩展 `WorkspacePrimarySurface`：

```ts
export type AppWorkspaceMode = "documents" | "feeds";
```

- [ ] RED 测试证明：进入 feeds 时 editor DOM 仍挂载但 `aria-hidden`/不可交互；返回后同一 editor node、Agent 意图、document tab 不变。
- [ ] `AppShell` 新增 `workspaceMode`、`feedWorkspace`、`onWorkspaceModeChange`；document main 与 feed main 只切可见性，不互相卸载。
- [ ] feeds 模式临时折叠 Agent 的有效 presentation，但不写回 `aiPanelOpen`；返回 documents 恢复原投影。
- [ ] 标题栏在笔记库入口旁增加 `Rss` 图标按钮，使用 `aria-pressed` 和中文 tooltip；点击文档 Tab 自动回 documents。
- [ ] 禅模式进入 feeds 前由 App 层退出并显示一次非阻断提示。
- [ ] 运行 AppShell 新旧测试，确保 v1.2.19 宽度预算无回归。
- [ ] 提交：`feat(ui): 增加不卸载编辑器的订阅工作区`。

### Task 4.3：构建导航、列表和阅读器

**Files:**

- Create: `src/components/feed/FeedWorkspace.tsx`
- Create: `src/components/feed/FeedSidebar.tsx`
- Create: `src/components/feed/FeedItemList.tsx`
- Create: `src/components/feed/FeedReader.tsx`
- Create: `tests/feed-workspace.test.tsx`
- Modify: `src/styles/globals.css`
- Modify: `docs/design-system.md`

- [ ] 先写组件 RED 测试：五个文章视图与同步失败源视图、未读计数、空态、loading/error、打开延迟已读、快捷键、批量已读、同步状态、远程图片默认阻止。
- [ ] `FeedItemList` 使用现有 `@tanstack/react-virtual`，稳定 key 为 item ID；不得复制虚拟化实现。
- [ ] `FeedReader` 正文应用 `--prose-measure`，标题聚焦，显示来源/日期/转换降级提示和外部打开动作。
- [ ] 宽屏来源导航可折叠；1024–1365 用抽屉；800–1023 使用列表/阅读单平面状态机。
- [ ] 未读同时用字重、圆点和 `aria-label`；不能只用 brand 色。
- [ ] 新增样式前先在 design-system 写 token/组件用法；优先复用现有 background/panel/border/brand token。
- [ ] 运行 `npm run test -- feed-workspace`、`npm run lint`、`npm run typecheck`。
- [ ] 提交：`feat(rss): 构建响应式订阅阅读工作区`。

### Task 4.4：添加订阅管理与搜索交互

**Files:**

- Create: `src/components/feed/FeedSourceDialog.tsx`
- Create: `tests/feed-source-management.test.tsx`

- [ ] RED 测试：URL 发现、多候选选择、历史未读选项、编辑标题/分组/间隔、暂停、两种退订、搜索 debounce/清空/分页/错误。
- [ ] 添加流程拆为「发现」和「确认订阅」；多候选不可自动全选。
- [ ] 删除订阅及文章显示计数并二次确认；保留文章选择实际将 source 置 disabled，不删除。
- [ ] 搜索 200ms debounce；输入法 composition 中不发请求；Escape 清空并回到先前视图。
- [ ] 同步失败提供「重试」和安全原因文案，不展示 URL/HTTP body/stack。
- [ ] 搜索框、source menu 和添加表单直接作为 `FeedWorkspace`/`FeedSourceDialog` 的局部组件；只有文件超过约 300 行且职责确实独立时再拆分。
- [ ] 运行对应测试，预期 GREEN。
- [ ] 提交：`feat(rss): 完成订阅管理与本地搜索体验`。

**阶段 4 退出条件：** 800×600 到 1920×1080 全尺寸可用；编辑器状态无损；键盘/读屏/主题/缩放通过；不发生远程图片被动请求。

## 阶段 5：OPML、退订可迁移性与保存为笔记

### Task 5.1：实现 OPML 导入导出

**Files:**

- Create: `src-tauri/src/feed/opml.rs`
- Create: `src-tauri/src/feed/opml_tests.rs`
- Modify: `src-tauri/src/commands/feed_commands.rs`
- Modify: `src/lib/ipc.ts`
- Create: `src/components/feed/FeedOpmlDialog.tsx`
- Create: `tests/feed-opml-dialog.test.tsx`

- [ ] RED 测试覆盖嵌套分组、重复 URL、缺字段、XXE、5 MiB 上限、UTF-8、导入幂等、导出→导入往返。
- [ ] OPML 输入通过 IPC 传有界 UTF-8 字符串，不让 Rust command 接收任意文件路径；文件选择/保存由前端现有 dialog 能力完成。
- [ ] 解析拒绝 DTD/ENTITY；只读取 `outline[text/title/xmlUrl/htmlUrl]`，忽略未知字段。
- [ ] 规范化 URL 后去重；已存在 source 默认更新 folder/title override 但不重置状态。
- [ ] 导出按 `folder_path` 稳定排序，不包含 ETag、错误、阅读状态或本地 ID。
- [ ] UI 在执行前预览新增/更新/跳过数量，并让用户选择历史是否未读。
- [ ] 运行 Rust/React OPML 测试。
- [ ] 提交：`feat(rss): 添加可往返的 OPML 导入导出`。

### Task 5.2：显式保存为 Vault 笔记

**Files:**

- Create: `src/lib/feed-note-export.ts`
- Create: `tests/feed-note-export.test.ts`
- Modify: `src/components/feed/FeedReader.tsx`
- Modify: `src/App.impl.tsx`

输出契约：

```markdown
# 文章标题

> 来源：[订阅源标题](https://example.com/feed-site)  
> 原文：[打开原文](https://example.com/article)  
> 发布：2026-08-11T08:00:00Z  
> 保存：2026-08-11T09:00:00Z

规范化后的文章正文
```

- [ ] RED 测试覆盖缺作者/日期/URL、危险标题字符、重复文件名、UTF-8、正文不被二次 HTML 解码。
- [ ] `buildFeedNoteMarkdown(detail, savedAt)` 只消费安全 DTO，不访问 raw payload。
- [ ] 点击「保存为笔记」必须明确选择目标目录/文件名；默认文件名经现有路径校验，不静默覆盖。
- [ ] App 层复用 `fileCreate` + `DocumentPersistenceCoordinator`/现有写盘回执；不得从 Feed component 直接 `invoke` 或 `fs.write`。
- [ ] 写盘成功后打开生成笔记；失败停留在文章并显示可重试错误。
- [ ] 保存后的笔记是独立副本；后续 Feed 更新不修改 `.md`，删除笔记不影响 Feed。
- [ ] 运行 note export、持久化协调器和文件生命周期测试。
- [ ] 提交：`feat(rss): 支持显式保存订阅文章为笔记`。

### Task 5.3：容量、升级与故障回归

**Files:**

- Create: `src-tauri/tests/feed_library_capacity.rs`
- Modify: `docs/testing/rss-subscription-library-manual-checklist.md`

- [ ] 用合成文本建立 100 个 source、10,000 个 item 的 integration test；断言 inbox 首屏、FTS 查询和详情读取正确，不保存机器相关的毫秒硬阈值。
- [ ] 用 `EXPLAIN QUERY PLAN` 断言 inbox 和 source 列表使用既有索引；只有查询确实全表扫描时才调整索引。
- [ ] 从加入 RSS 前的应用数据库副本启动，确认 `063` 自动应用且现有笔记、会话、设置不变。
- [ ] 手工验证断网、代理切换、DNS/证书失败、429/500、超时、超限和磁盘满；旧文章始终可读。
- [ ] 验证应用退出后无需恢复 job 状态；重启只按 `next_fetch_at` 或手动刷新继续。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml --test feed_library_capacity`，预期 GREEN。
- [ ] 提交：`test(rss): 添加订阅容量与升级回归`。

### Task 5.4：全量自动化质量门禁

按顺序执行并保存 exit code；任一失败不得声称完成：

- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml`
- [ ] `npm run audit:rust`
- [ ] `npm run lint`
- [ ] `npm run format:check`
- [ ] `npm run typecheck`
- [ ] `npm run test`
- [ ] `npm audit`
- [ ] `npm run docs:check`
- [ ] `npm run test:e2e`
- [ ] 发布候选另跑 `npm run tauri build` 与仓库既有 macOS/Windows 发布门禁。

预期：全部 exit 0；审计无未登记高危；E2E 不依赖公网源，使用本地受控 HTTPS fixture server 或 mock transport。

### Task 5.5：macOS、Windows 与全尺寸人工验收

**Files:**

- Modify: `docs/testing/rss-subscription-library-manual-checklist.md`
- Modify: `CHANGELOG.md`（只有实际交付时）
- Modify: `ARCHITECTURE.md`（只有实际存在时）
- Modify: `ROADMAP.md`

- [ ] macOS、Windows 各验证冷启动、添加、同步、离线阅读、OPML、外链、保存笔记、应用重启。
- [ ] 完成全尺寸/主题/缩放/键盘/读屏/reduced-motion 矩阵并附日期、构建、平台证据。
- [ ] 验证 800×600 无遮挡、1366 不强制三栏、1920 正文不无限拉宽。
- [ ] 验证远程图片默认零请求，用户加载后只请求 HTTPS 且 no-referrer。
- [ ] 验证日志抽样无 URL、标题、正文、OPML 或请求头。
- [ ] 实际功能完成后才更新 ARCHITECTURE/CHANGELOG；ROADMAP 状态改为「已交付」必须引用全部门禁证据。
- [ ] 由项目所有者完成最终 diff 审查；修复后重跑 Task 5.4 的全量门禁。
- [ ] 最终提交：`feat(rss): 交付本地优先的订阅资料库`。

**阶段 5 退出条件：** OPML 往返与保存笔记通过；自动化全绿；macOS/Windows 和全尺寸清单完成；升级/故障回归完成；没有发布阻断，文档与代码一致。

---

## 后续路线（不属于本计划的完成条件）

以下工作必须另写规格和实施计划，不得顺手扩张首轮：

1. FreshRSS 或 Miniflux 同步；只有真实需求出现后再选择其一设计。
2. 合法网页全文抓取与 Readability。
3. 订阅内容进入全局搜索的分组结果。
4. 用户逐 Run 授权 Agent 读取订阅资料库。
5. 公众号搜索服务的合法数据源、许可、账号风险和可持续性评估。

## 自审记录

- 规格覆盖：存储、转换、收件箱、同步、搜索、UI、MCP 取舍、迁移、测试、发布、回滚均有对应 Task。
- 类型一致性：Rust DTO 使用 camelCase；TypeScript 不暴露 raw payload；三个阅读状态为独立布尔 patch/时间戳。
- 实体克制：只有 `feed_sources`、`feed_items` 和派生 FTS；没有 shelves、jobs、provider、tag 或 inbox 表。
- 既有能力复用：系统代理、安全 HTTPS 校验、外链 opener、SQLite、虚拟列表、Markdown renderer、DOMPurify、文档持久化协调器。
- 占位检查：本文不含待补步骤或伪路径；后续范围明确标为另立计划，不是当前施工占位。
