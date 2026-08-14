# RSS 订阅资料库产品与架构规范

> 状态：已冻结（2026-08-11）  
> 排期事实以 [ROADMAP.md](../ROADMAP.md) 为准；本文定义产品边界、数据契约与验收口径，不表示功能已经交付。

## 1. 结论

Iris 应把 RSS 建成一个独立的「订阅资料库」，而不是把每篇订阅内容自动写成 Vault 中的 `.md` 文件，也不应以 MCP 作为基础运行时。

- 订阅源、文章、同步游标和阅读状态属于应用状态，保存在应用级 SQLite。
- 文章正文以 Markdown 作为 Iris 内部的规范化阅读表示，同时保留仅后端可访问的原始源载荷与转换版本，以便修复转换器后重新生成。
- 「收件箱」是 `未归档` 的动态视图；已读只表示阅读状态，不自动移出收件箱。
- RSS 阅读器首期不接入 Agent、RAG、联网搜索或通用 MCP；这些能力以后只能通过单独授权、单独索引和单独验收进入。
- 用户明确执行「保存为笔记」时，才通过现有 `fileCreate` 写盘回执与 `openNote` 路径把一份可读 Markdown 副本写入当前 Vault。

## 2. 产品定位与范围

### 2.1 用户目标

1. 添加标准 RSS、Atom 或 JSON Feed，稳定接收新内容。
2. 在 Iris 内以一致、可搜索的 Markdown 阅读体验查看正文。
3. 用收件箱、全部、今日、收藏、归档和同步失败视图管理阅读流。
4. 在离线状态下阅读已经同步的内容。
5. 通过 OPML 导入和导出迁移订阅关系，不被 Iris 锁定。
6. 在确有长期价值时，将单篇文章显式保存为 Vault 笔记。

### 2.2 首轮完整交付范围

- RSS 0.x/1.0/2.0、Atom、JSON Feed 解析。
- 订阅发现、添加、编辑、暂停、删除。
- 手动同步、定时增量同步、条件请求、失败退避。
- 文章去重、更新、Markdown 转换、全文检索。
- 未读、已读、收藏、归档三个互相独立的状态轴。
- 派生收件箱及批量已读。
- OPML 导入/导出。
- 响应式订阅工作区、键盘与读屏基础能力。
- 显式「保存为笔记」。

### 2.3 明确不在首轮范围

- Agent、RAG、Embedding、AI 摘要、AI 标签或自动写入笔记。
- 微信客户端内搜索、公众号历史库抓取、绕过登录/反爬或版权限制。
- 付费墙处理、浏览器扩展、按站点维护的正文规则或任何绕过登录/反爬机制。
- Podcast 播放、视频下载、附件离线缓存。
- FreshRSS/Miniflux 双向同步、Google Reader/Fever API。
- RSS MCP、任意第三方脚本或插件市场。
- 多设备云同步、多人协作。
- 自动绕过保留策略或把 RSS 回收站混入 Markdown 笔记回收站。

微信公众号内容只有在上游能提供合法、稳定 Feed 时才能进入 Iris。RSS 基础设施提升的是订阅、沉淀与本地检索能力，不等价于获得微信全量搜索能力。

### 2.4 单人项目施工原则

- 订阅业务事实仍只使用 `feed_sources`、`feed_items` 两张表；历史边界、保留期、回收站和正文状态均为其字段，不新增同步 job 或全文业务表。
- 复用现有 Scheduler、HTTP/代理策略、SQLite、Markdown 渲染、虚拟列表和文档持久化链路。
- 不建设 provider 框架、同步 job 表、遥测系统或插件接口；抽屉复用现有 Radix Dialog 依赖的共享 Sheet 原语。
- RSS 解析与 HTML → Markdown 使用精确锁定的 `feed-rs`、`quick-xml`、`htmd`；安全固定目标传输复用锁文件中已有的 `hyper`、`tokio-rustls` 与平台证书验证链。OPML 文件选择与读写使用官方 Tauri dialog/fs 能力；这些依赖均须如实记录许可与用途，不能再表述为“只有两个新增依赖”。
- 容量回归按个人资料库的 100 个订阅源、10,000 篇文章设计，不以企业级聚合服务为目标。

## 3. 信息架构

顶层入口名称使用「订阅」，资料库品牌文案可以使用「藏书阁」，但导航和辅助功能使用明确的功能名。

```text
订阅
├── 收件箱（未归档；已读仍保留）
├── 今日（本地时区当天收到）
├── 全部
├── 收藏
├── 归档
├── 同步失败
└── 订阅源
    └── folder_path 分组（来自 OPML 大纲或用户设置）
```

首轮不增加 `feed_shelves`、`feed_folders` 或标签关系表。分组用订阅源上的 `folder_path` 表达；当确实出现重命名、嵌套权限或跨源多归属需求时，再通过 migration 升级为实体。

## 4. 数据所有权与存储边界

| 数据               | 权威来源                | 存储位置         | 删除后能否重建                 |
| ------------------ | ----------------------- | ---------------- | ------------------------------ |
| 用户笔记           | Vault `.md`             | 用户 Vault       | 是，SQLite 索引可重建          |
| 订阅配置           | Iris 应用状态           | 应用级 `iris.db` | 否；可由 OPML 恢复部分字段     |
| 订阅文章           | 上游 Feed 的本地快照    | 应用级 `iris.db` | 不保证；上游可能已删除         |
| Feed 文章 Markdown | Feed 源载荷的规范化结果 | 应用级 `iris.db` | 可由保留的 Feed 源载荷重转     |
| 网页补全 Markdown  | 有界的网页正文缓存      | 应用级 `iris.db` | 删除后仅可在启用补全时重新抓取 |
| 未读/收藏/归档     | Iris 应用状态           | 应用级 `iris.db` | 否                             |
| 保存后的文章笔记   | Vault `.md`             | 当前 Vault       | 是；此后与订阅文章独立         |

订阅资料库跨 Vault 共享。切换 Vault 不复制订阅或重置阅读状态；只有「保存为笔记」需要当前 Vault，并必须经过现有写盘、冲突和索引回执链路。首轮仍从已选择 Vault 后的桌面壳层进入「订阅」，不改造当前启动门禁；数据归属与入口时机是两个独立问题。

## 5. 最小数据模型

### 5.1 `feed_sources`

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
    history_boundary_external_key TEXT,
    history_boundary_published_at TEXT,
    fulltext_enabled INTEGER NOT NULL DEFAULT 0
        CHECK (fulltext_enabled IN (0, 1)),
    deleted_at TEXT,
    purge_after TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

`fulltext_enabled` 的 schema 默认值保留为兼容旧版本的 `0`；Repository 创建新来源时显式写入 `1`，而 `066_feed_fulltext_default_on` 将升级前的既有来源统一切换为 `1`。这样不会依赖 SQLite 历史默认值，也不会改写旧文章的 `fulltext_status`。

显示标题使用 `COALESCE(title_override, title)`。错误只保存稳定错误码和时间，不保存 URL、响应正文、Token 或文章内容。

### 5.2 `feed_items`

```sql
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
    expires_at TEXT,
    deleted_at TEXT,
    purge_after TEXT,
    content_origin TEXT NOT NULL DEFAULT 'feed'
        CHECK (content_origin IN ('feed', 'web')),
    fulltext_status TEXT NOT NULL DEFAULT 'not_requested'
        CHECK (fulltext_status IN ('not_requested', 'pending', 'fetching', 'ready', 'failed')),
    fulltext_markdown TEXT,
    fulltext_extraction_version INTEGER NOT NULL DEFAULT 0,
    primary_document_kind TEXT,
    primary_document_url TEXT,
    deletion_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (source_id, external_key)
);
```

`source_payload` 是经 Feed parser 清理、但尚未转换为 Markdown 的条目级载荷，不是完整 HTTP 响应的逐字节副本。它永不通过 IPC 返回前端，用于重转和问题诊断，不能进入日志、错误文本或遥测。`fulltext_markdown` 仅保存安全提取出的有界 Markdown；原始网页 HTML、Cookie、代理地址和底层网络错误不落盘。SQLite 数据库继续沿用 Iris 应用数据目录的访问边界；首轮不另造专有文件容器。

### 5.3 `feed_items_fts`

使用 SQLite FTS5 external-content 表索引 `title`、`author_name`、`content_text`，通过 INSERT/UPDATE/DELETE trigger 与 `feed_items.row_id` 同步。搜索只返回经过长度限制的转义摘要，不返回源 HTML。

### 5.4 不建模的字段

- enclosure、Media RSS 和播客信息首轮不持久化。
- Feed 分类先投影到 `folder_path`，不建多对多标签表。
- 同步任务只保存在内存协调器中，不建 `feed_sync_jobs`；崩溃后按 `next_fetch_at` 恢复。
- 收件箱、今日和失败均为查询，不建复制表。

## 6. 内容标准化

### 6.1 选择正文

按以下确定性顺序选择每个条目的主内容：

1. 非空 `entry.content.body`；
2. 非空 `entry.summary.content`；
3. 标题与规范链接组成的最小 Markdown。

内容类型以 Feed 声明为主，未知或矛盾时按纯文本安全降级，不能猜成可执行 HTML。

### 6.2 转换流水线

```text
有界响应字节
  → 拒绝 DTD/ENTITY
  → feed-rs 解析并清理 HTML 字段
  → 选择正文与稳定标识
  → HTML/XHTML 使用 htmd 转 Markdown；纯文本转义
  → 规范化链接、空白、标题和图片
  → 生成 content_text / SHA-256 / conversion_version
  → SQLite 事务 upsert
  → 前端 marked 渲染
  → DOMPurify 二次净化
```

Rust 候选依赖锁定前必须完成许可、MSRV、审计与产物体积评估：

- [`feed-rs`](https://docs.rs/crate/feed-rs/latest) 支持 RSS、Atom 与 JSON Feed，MIT；启用其 `sanitize` feature。
- [`quick-xml`](https://docs.rs/crate/quick-xml/latest) 精确锁定 `0.41.x`，用于有界 OPML 属性解析，MIT；避免手写 XML 扫描。
- [`htmd`](https://docs.rs/crate/htmd/latest) 负责 HTML → Markdown，Apache-2.0。
- [`hyper`](https://docs.rs/crate/hyper/latest)、`tokio-rustls` 与平台证书验证链只用于安全 HTTPS GET：在 HTTP/1 解析阶段限制响应头，并把经 DNS 校验的 IP 固定写入直连、HTTP CONNECT 或 SOCKS5 隧道；均为 MIT 或 Apache-2.0 兼容许可。Tauri 官方 `dialog`/`fs` 插件仅用于用户显式选择的 OPML 文本文件。

网页正文补全与 Feed 同用 Iris 唯一的“使用系统代理”设置。安全传输仅支持无认证 HTTP CONNECT 与 SOCKS5，并把本地校验后的 IP 写入隧道目标；PAC/自动发现、HTTPS-to-proxy 与认证代理返回稳定失败码，绝不静默直连或把域名交给代理解析。

网页正文补全不是针对任何站点的爬虫规则，而是默认阅读能力。新来源以及升级后的既有来源默认开启；对后续仅含摘要且仍在保留期内的条目，Iris 使用同一条安全 HTTPS 管道请求文章链接。升级前已经保存的摘要不会静默批量抓取：用户打开一篇时才只排队这一篇。静态 HTML 按通用语义容器（`article`、`main` 等）抽取并转为 Markdown；用户仍可在单个来源关闭自动补全。最多两个任务并发、同站点至少间隔 2 秒、文章响应上限 1 MiB、提取后的 Markdown 上限 768 KiB。不会执行 JavaScript、绕过登录/付费墙或保存原始 HTML；失败时始终保留 Feed 摘要和原文链接。

前端不使用 TipTap 编辑订阅正文；复用只读 Markdown 渲染与现有 `marked`、DOMPurify 能力。这样正文可读、可复制、可搜索，同时不把外部文章伪装成用户笔记。

### 6.3 Markdown 规范

- 输出 UTF-8、LF 换行，末尾保留一个换行。
- 保留标准标题、段落、列表、引用、代码、表格、链接和图片。
- 删除 `script`、`style`、`iframe`、表单、事件属性、内联样式和未知可执行节点。
- 相对链接以文章规范 URL 为基准转成绝对 HTTPS URL；无法安全解析的链接降级为纯文本。
- `javascript:`、`data:`、`file:`、`asset:` 等来源链接不得进入文章 Markdown。
- 远程图片默认不请求，只显示占位与「加载本篇图片」动作。授权仅作用于当前文章并持久到文章内容更新、回收或缓存清理；授权命令只返回图片清单，前端按视口前后 600px 渐进请求，后端最多同时下载两张，单张失败不阻塞正文或其它图片。二进制写入 `IRIS_CACHE_DIR/feed-media/images`，使用唯一 `.part`、活动租约和原子重命名；近期或活动的 `.part` 不会被其它下载的维护任务删除。为兼容要求来源页面的图片 CDN，只有用户明确授权当前文章时，后端可发送已保存文章的 HTTPS 规范 URL（移除 query/fragment）作为最小 `Referer`；除此以外不发送 Referer，永不携带 Cookie、前端请求头、浏览历史或代理地址。前端本地图片仍使用 `lazy loading` 与 `referrerPolicy=no-referrer`，不提供来源级、全局或会话级永久放行。
- 转换失败不丢条目：以转义后的纯文本保存，标记 `conversion_status=degraded`。

### 6.4 重转策略

`conversion_version` 是代码常量。基础版只记录版本，不提前实现后台重转器。将来确实修改转换规则时再新增有界重转 migration/任务，并保持 `read_at`、`starred_at`、`archived_at`、`received_at` 和稳定 `id` 不变。

### 6.5 默认网页正文补全

普通订阅优先显示 Feed 提供的正文；仅有摘要时，来源默认会自动补全网页正文。历史内容不会在升级后自动批量抓取：打开旧摘要会只排队当前一篇；同一文章重复打开会复用队列。用户可在单个来源关闭“自动补全网页正文”。

补全器不含站点白名单或域名特例。它先读取通用 `citation_*`、Dublin Core、OpenGraph 等元数据，再对 `article/main/role` 等候选按正文、链接与控件密度评分；不可靠的 `body` 不会成为正文。动态页面、登录页、付费墙、非 HTML 或提取失败时保留 Feed 摘要和原文链接。提取规则带版本；旧版网页正文只在用户再次打开单篇时重取，失败即清除旧网页壳并回退摘要，不后台扫描历史。每项复用安全 HTTPS、唯一系统代理、逐跳 DNS pinning 与固定 IP CONNECT/SOCKS5；响应体最多 1 MiB、转换 Markdown 最多 768 KiB、同一时间最多两项、同站请求至少相隔两秒。只保存有界 Markdown 与用于 FTS 的纯文本，绝不保存网页 HTML、Cookie 或代理信息。

通用学术元数据可声明一个 PDF 主文档。PDF 只在用户点击后下载：不同文档全局一次只下载一项，同一规范 URL 的重复请求共享该下载；等待、连接、重定向和流式写入合计受 180 秒限制，取消会同时唤醒排队与下载中的任务。单文件最多 100 MiB，图片与 PDF 分别最多 512 MiB、30 天未访问淘汰（合计默认不超过约 1 GiB），固定 64 KiB 流式写入随机 `.part` 后原子重命名；响应必须同时通过 HTTPS、类型与 `%PDF-` 文件头校验。前端只持有短期 opaque lease，不接触真实路径；仍持有 lease 的文件不得被 LRU、显式清理或来源清理删除。缓存不进入 Vault、索引、Agent 或 RAG。

## 7. 同步与去重

### 7.1 发现与订阅

用户可输入 Feed URL 或站点 URL。发现过程只读取 HTTPS 公开地址：

- 如果响应本身可解析为 Feed，直接返回候选。
- 如果是 HTML，仅解析 `<link rel="alternate">` 的 RSS/Atom/JSON Feed 候选。
- 多个候选必须让用户选择；不能默默订阅全部。
- 首次同步默认把历史项目标为已读，只有同步后首次出现的新项目进入收件箱；添加时可显式选择「历史也设为未读」。

### 7.2 条件请求和调度

- 保存并发送 `ETag` / `If-None-Match`、`Last-Modified` / `If-Modified-Since`。
- `304` 更新检查和下次同步时间，不触碰文章。
- 默认间隔 60 分钟；最短 15 分钟，最长 7 天。
- 复用现有 Scheduler 每 15 分钟扫描一次到期源；单轮最多 2 个并发，不新增独立任务系统。
- 瞬态失败按 15 分钟、1 小时、6 小时、24 小时退避，不加入难以诊断的随机策略。
- 手动刷新可绕过 `next_fetch_at`，但不能绕过并发锁、地址校验和响应上限。

### 7.3 稳定键

`external_key` 按以下顺序生成：

1. Feed 条目稳定 ID/GUID；
2. 规范化后的文章 URL；
3. `source_id + title + published_at` 的 SHA-256。

同一 `(source_id, external_key)` 只保留一行。正文 hash 改变时更新内容和源更新时间，但保留用户阅读状态；URL 或标题变化不制造新未读。不同订阅源的同一文章首轮不跨源合并，以免错误吞掉镜像源和用户状态。

### 7.4 删除与退订

- 上游删除条目不自动删除本地副本。
- 退订分为「暂停同步」和「移入 RSS 回收站」。前者经 `feed_source_update` 设置 `isEnabled=false`；后者将来源及本次退订文章保留 30 天。恢复后默认暂停，且不得恢复此前因保留期限删除的文章。
- 删除是破坏性动作，需要显示文章数、收藏数和计划清理日期并二次确认；30 天内重新添加相同规范 URL 时必须提示“恢复并重新订阅”，不得静默恢复或创建重复来源。
- 未归档文章保留 7 天，归档文章保留 30 天，收藏文章永久保留；到期后先移入 RSS 专属回收站，30 天后物理清理。此规则不影响 Markdown 笔记回收站。

## 8. 阅读状态与收件箱

三个时间戳互不覆盖：

- `read_at`：为空表示未读；重新标未读时置空。
- `starred_at`：为空表示未收藏。
- `archived_at`：为空表示未归档。

派生文章视图：

```sql
-- 收件箱
WHERE archived_at IS NULL

-- 收藏
WHERE starred_at IS NOT NULL

-- 归档
WHERE archived_at IS NOT NULL
```

「同步失败」不是文章状态，而是 `feed_sources.last_error_code IS NOT NULL` 的订阅源诊断视图；重试成功后自动退出该视图。

打开文章后不立即标已读；正文可见 1 秒或用户执行滚动/键盘阅读动作后标记，避免列表预览误清未读。用户可关闭自动已读并改用显式操作。批量已读必须基于当前冻结筛选条件执行，并返回影响行数。

## 9. 搜索

- 首期搜索域仅为订阅资料库，不混入笔记结果页。
- 查询源标题、文章标题、作者和 `content_text`；支持按视图和订阅源过滤。
- 使用 FTS5 `unicode61`，中文基础体验以字符/短语匹配验收；不引入 jieba 或新的分词运行时。
- 结果展示来源、发布时间、标题和净化摘要。
- 搜索不触发联网，不读取尚未同步的网页，不把文章送给 Agent。
- 后续若进入全局搜索，必须以「笔记 / 订阅」分组并明确数据来源，不能把订阅文章伪装成 Vault 文档。

## 10. 前端工作区

### 10.1 进入与退出

标题栏在笔记库入口旁增加「订阅」入口。订阅是与文档工作区并列的应用工作区模式，不是文档 Tab，也不是 Overlay。进入订阅时：

- 编辑器和 Agent 会话保持挂载；不得丢失 dirty、光标、选区、滚动或 undo。
- 默认折叠 Agent 侧车，避免形成永久三栏；退出订阅恢复用户原有 Agent 意图。
- Tab rail 保留但进入只读的工作区切换表现，用户选择文档 Tab 即返回文档模式。
- 禅模式只服务写作，进入订阅前退出禅模式并提示一次。

### 10.2 响应式布局

| 内容宽度      | 布局                                                                 |
| ------------- | -------------------------------------------------------------------- |
| `>= 1366px`   | 可折叠来源导航 + 文章列表 + 阅读区；来源导航默认收起，不形成永久三栏 |
| `1024–1365px` | 文章列表 + 阅读区；来源与视图用抽屉                                  |
| `800–1023px`  | 单平面列表/阅读切换；返回键和面包屑明确                              |

最小支持窗口沿用 Tauri 的 `800 × 600`。正文宽度复用 `--prose-measure`，列表使用虚拟化；不因长标题撑破布局。

来源抽屉从统一标题栏下方开始，不能覆盖 macOS traffic lights。`1024–1365px` 由抽屉外框提供唯一右边界；`800–1023px` 抽屉占满可用工作区且无右侧空白轨。抽屉内 `FeedSidebar` 必须占满容器、移除自身右边框，关闭按钮与“添加订阅”并列在既有“订阅”顶栏，禁止重复标题行。

### 10.3 组件边界

```text
src/components/feed/
├── FeedWorkspace.tsx
├── FeedSidebar.tsx
├── FeedItemList.tsx
├── FeedReader.tsx
└── FeedSourceDialog.tsx
```

业务数据读取放在 `src/hooks/useFeedLibrary.ts`，安全渲染放在 `src/lib/feed-reader.ts`；简单行项目、空态、工具栏和状态提示先作为上述组件的局部组件，不为文件数量而拆分。`components/ui/` 不得加入 RSS 业务逻辑。

### 10.4 交互与可访问性

- `j/k` 移动列表，`o/Enter` 打开，`m` 已读/未读，`s` 收藏，`e` 归档，`r` 刷新；输入框聚焦时不触发。
- 列表使用 roving tabindex 或等价的单一键盘焦点，阅读区标题作为打开后的焦点目标。
- 所有图标按钮有中文 accessible name；未读不能仅靠颜色表达。
- 支持亮/暗主题、200% 缩放、`prefers-reduced-motion` 和读屏状态播报。
- 同步错误显示稳定、可行动文案，不显示原始响应或内部堆栈。

## 11. 安全与隐私

### 11.1 网络边界

- 首轮只接受 HTTPS Feed 与发现 URL。
- 禁止 userinfo、localhost、loopback、private、link-local、unspecified、multicast、metadata 地址和私有域提示。
- DNS 解析后校验全部地址并固定本次连接；每次重定向重新解析和校验，最多 5 跳。
- 使用现有「跟随系统代理」设置；关闭时强制直连。
- 连接/总超时、响应头长度和流式字节数均有上限：Feed 5 MiB、发现页 2 MiB、OPML 5 MiB。
- 拒绝 XML DTD/ENTITY；解析在有界字节缓冲中完成。

### 11.2 展示边界

- IPC DTO 只包含净化后的 Markdown/纯文本和安全元数据。
- 前端 `marked` 输出必须再过专用 DOMPurify allowlist。
- 外链只能通过现有 `open_external_https_url` 打开。
- 远程图片默认阻止，避免打开文章即泄露 IP、User-Agent 或阅读时间。
- 用户点击「加载本篇图片」后，Iris 仅从该文章保存的 Markdown 提取 HTTPS 图片，
  经安全后端下载到独立缓存；为兼容防盗链，后端仅可携带该文章去除 query/fragment 的
  HTTPS URL 作为最小 Referer，绝不带 Cookie、前端请求头或用户浏览信息；WebView 使用
  `iris-feed-image` opaque lease 读取本地文件，继续保持 no-referrer。
  授权按文章持久到内容/网页正文变更、移入回收站或缓存清理，切换文章和重启不会回退为热链。
- 不执行源内脚本、样式、iframe、表单、媒体自动播放或自定义协议。

### 11.3 日志与诊断

允许记录：稳定源 ID、条目数量、耗时、HTTP 状态类别、稳定错误码。禁止记录：Feed URL、文章 URL、标题、正文、源载荷、请求/响应头和用户 OPML 内容。

## 12. IPC 边界

计划新增一组独立 `feed_*` 命令：

- `feed_discover`
- `feed_source_add` / `feed_source_list` / `feed_source_update` / `feed_source_trash`
- `feed_item_list` / `feed_item_get` / `feed_item_set_state` / `feed_items_mark_read`
- `feed_sync_source` / `feed_sync_all` / `feed_sync_batch`
- `feed_opml_import` / `feed_opml_export`

同步事件只投影 `sourceId`、变更类型、计数和稳定错误码，用于通知前端重新查询；不建设 job 恢复协议。所有 IPC 同步更新 Rust command、`src/types/ipc.ts`、`src/lib/ipc.ts`、事件类型、测试与 `docs/ipc-api-reference.md`。

Feed 与 AI 网页安全抓取复用唯一的「使用系统代理」设置。每跳先在本地解析并拒绝任一私网地址，再将 HTTP CONNECT authority 或 SOCKS5 地址类型固定为已验证 IP；TLS SNI、证书校验和 HTTP Host 保持原域名。关闭全局代理时安全直连；PAC、HTTPS-to-proxy、认证代理、不可达代理均稳定失败，绝不静默回退直连。

管理中心的一级「订阅」页只承载全局策略（自动已读、后台同步、后续新源默认间隔）和资料库维护摘要/全量同步/OPML 入口；单源标题、分组、暂停与退订始终留在订阅工作区，避免两套来源管理。

## 13. 可选外部设施与 MCP 决策

### 13.1 基础版不依赖 MCP

RSS MCP 通常围绕「让模型临时列出/读取 Feed」设计，不能替代离线内容、阅读状态、条件请求、迁移、全文检索和无 Agent 使用场景。把它作为基础会让用户数据受第三方工具生命周期影响，并与 Iris 当前逐 Run `external.read` 授权模型冲突。

### 13.2 后续可选服务适配

- [FreshRSS](https://github.com/FreshRSS/FreshRSS)（AGPL-3.0）可通过 Google Reader API 作为自托管同步源。
- [Miniflux](https://github.com/miniflux/v2)（Apache-2.0）提供 REST API，但服务端本身依赖 Go/PostgreSQL，不应嵌入 Iris。

如果原生单机路径长期稳定且确有多设备同步需求，再为其中一个服务单独立项。首轮不创建 `FeedProvider` trait、provider 表、凭据字段或通用适配层。

## 14. 阶段门禁

| 阶段             | 可交付能力                              | 进入下一阶段的硬门禁                  |
| ---------------- | --------------------------------------- | ------------------------------------- |
| 0 契约与 fixture | 产品决定、格式/安全语料、依赖评审       | 关键决定确认；许可/MSRV/审计通过      |
| 1 本地资料库     | migration、repository、FTS、状态机      | up/down/idempotent；不触碰 Vault      |
| 2 同步核心       | 安全抓取、Markdown、去重、手动/自动同步 | 格式、SSRF/XXE、304、状态保持通过     |
| 3 IPC            | 类型安全的订阅读写与同步命令            | Rust/TypeScript/文档契约一致          |
| 4 阅读工作区     | 收件箱、列表、阅读、搜索、状态操作      | 800–1920 布局、a11y、主题、键盘通过   |
| 5 迁移与发布     | OPML、保存为笔记、升级、回滚            | 全量质量命令及 macOS/Windows 验收通过 |

## 15. 产品决策点

以下六项决策已于 2026-08-11 经项目所有者逐项确认并冻结，不再作为施工期可调项：

1. 订阅资料库全局共享，不按 Vault 隔离。
2. 首次同步历史默认已读。
3. 远程图片默认阻止。
4. 首轮只接受 HTTPS。
5. 未归档文章保留 7 天、归档文章保留 30 天、收藏永久保留；到期先进入 RSS 专属回收站并在 30 天后物理清理。
6. 保存为笔记生成独立副本，不与订阅条目双向同步。

其中第 1 项若改为按 Vault 隔离，会改变数据库键、切库生命周期、OPML 边界和 UI 状态；该结构性决策已在开始 migration 前冻结。
