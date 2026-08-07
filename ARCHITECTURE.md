# Iris 架构

## Compatibility boundaries

Iris keeps compatibility only at read boundaries. Each adapter is one-way: current writes never dual-write or fall back to an old representation.

- CEF v1/v2 payloads are read, validated, converted to the current Run schema, and atomically rewritten. New Runs write only the current schema.
- Legacy Skills (`trigger` metadata or no manifest) are normalized to the current `SkillEntry` while scanning. A manifest draft requires explicit user action; vault `SKILL.md` is never auto-rewritten.
- Legacy `frontmatter.title`, placeholder names, and old localStorage persona are read mappings only. New notes use the filename/current SQLite profile; localStorage clears only after SQLite persistence succeeds.
- A stable document session id is allocated before normal editor entry. Recovery fixtures may use a recovery id, but normal paths never use `legacy:${path}` identity.

These adapters remain until a separately announced, reversible migration boundary. Their exit requires explicit user communication; they are not a second write path.

## Markdown document boundary

The persisted boundary is `Markdown file -> frontmatter/title separation -> Preserve-aware editor ingest -> TipTap/ProseMirror document -> ProseMirror Markdown serializer -> Markdown file`. Unsupported syntax becomes a Preserve node carrying its original source. A serializer failure is recoverable and does not fall back to HTML/Turndown or overwrite committed Markdown.

The current editor ingress still uses its isolated Marked renderer internally to prepare TipTap HTML; it is not a second persistence path. `marked` also serves AI messages and read-only Markdown display. The editor persistence path does not call `getHTML()` or Turndown. Replacing editor ingress with a ProseMirror MarkdownParser requires a complete custom-node parser and corpus migration before it can be claimed as complete.

> 本文描述当前已实现的边界，不承诺版本排期；版本排期唯一来源是 [ROADMAP.md](./ROADMAP.md)。

## 分层

```text
React 19 UI
  └─ src/lib/ipc.ts（类型安全 IPC 封装）
       └─ Tauri commands（DTO、鉴权和输入校验）
            └─ AI Run Runtime / 文件、索引、搜索、版本与回收站
                 └─ SQLite / 本地加密凭据 / Vault 文件系统
```

- `src/`：React、TipTap、状态和类型安全 IPC 调用；组件不直接调用 `invoke()`。
- `src-tauri/src/commands/`：Tauri 命令边界、DTO 和输入校验；不承载运行生命周期。
- `src-tauri/src/ai_runtime/`：唯一的 Run 生命周期、策略决策、显式上下文、证据账本、模型网关和工具能力。
- `src-tauri/src/indexer/`：Markdown/frontmatter、分块、链接、标签和索引更新。
- `src-tauri/src/storage/`：SQLite、增量迁移、FTS 与可选 sqlite-vec 注册。

## 自适应工作区

布局由纯策略模块 `src/lib/workspace-chrome-layout.ts` 决定：把用户意图（`aiPanelOpen`、`navigatorOpen`、`pinPreferred`、`primarySurface`、`zenMode`）与实测尺寸（内容宽度、根字号、`--prose-measure` 计算值）投影为有效 presentation（`document/assistant_focus` × `closed/peek/pinned` × `sidecar/collapsed/focus`）。resize 只改实测尺寸，不改写用户意图；文档保护宽度（52rem）在任何降级路径都不被突破。`useWorkspaceChromeLayout` 用 ResizeObserver 读取内容区宽度与 `--prose-measure`，只持久化 Agent 宽度（`iris.aiPanelWidth`）、导航固定偏好（`iris.workspaceChrome.navigatorPinPreferred`）与安全目录展开标识（无安全 vault identity 时仅进程内）。

`AppShell` 持有唯一布局实例：Agent 与 editor 始终是同一个 sibling 子树（`sidecar ↔ focus`、zen 进出均不 remount）；`assistant_focus` 时 editor 保持挂载但不可交互（`aria-hidden + invisible + pointer-events-none`），Agent 面板 `absolute inset-0` 覆盖主区；导航 slot 只做 `closed/peek/pinned` placement。布局动作经 `WorkspaceChromeActionsContext`（`useWorkspaceChromeActions`）下发；标题栏入口与 `Ctrl/Cmd+\` 快捷键在 Context 外，经 window CustomEvent（`workspace-chrome-events.ts`）转发。`Ctrl/Cmd+Shift+E` 保持打开管理中心的完整"浏览笔记库"。

文件导航与动作由共享 controller 提供：`useVaultCatalog`（catalog 加载/refresh/外部 watcher epoch/索引降级）与 `useVaultFileActions`（新建、重命名、移动、锁定、移入回收站，全部经 `useNavigatorFileLifecycle` 的 dirty flush 与路径迁移屏障、索引回执与提交回执）。管理中心 `VaultNavigatorBody` 与轻量 `WorkspaceNavigator`（`peek/pinned` placement、单列 folder/file 树、键盘语义、brand marker、`IrisSurfaceMenu` 行操作）共同消费这两个 controller；批量操作、语料库、模板与回收站恢复仍只留在管理中心。Agent 主区阅读（`assistant_focus`）的内容列统一使用 `.ai-focus-column`（最大 `var(--ai-focus-measure)`，70rem），消息流、确认区、授权边界、选中消息操作条与 Composer 同列。

## 数据原则

用户 `.md` 是笔记唯一权威来源。`files`、`chunks`、`links`、FTS 与嵌入索引均可由 Vault 重建；会话、Run、网页缓存和收件箱属于应用状态。应用不会在未确认时改写用户笔记。

## Agent Run

normal-domain Run 通过 `assistant_run_start`、`assistant_run_control` 和 `assistant_run_get` 执行、控制和回放。每个 normal-domain Run 在 accepted 后持久化，再进行策略、上下文、路由与 provider 调度；`assistant:run_event` 是唯一的前端生命周期事件，断流使用 `assistant_run_get` 回放。

`agent_run_events` 是追加式、安全的过程回放日志，而不是可据以重建全部 Run 的执行日志：事件不包含工具参数或原始输出，只保存稳定 capability、调用 ID、受限摘要、状态和安全错误码。`assistant_run_get` 的回放仅恢复安全快照与过程展示；Direct 与 ToolLoop 不支持进程级续跑，进程中断后不会从事件重新执行模型或工具。只有 Durable Run 才具有暂停与检查点语义，且其可恢复写入闭环仍须通过冻结计划、确认和内容 hash 复核。

会话通过不透明 `AssistantSessionRef` 寻址，并按 normal/classified 安全域物理隔离。涉密 Run 仅在当前进程内易失执行：解锁文档、prompt 与模型输出以 `Zeroizing` 保存，不拥有 SQLite 或 CEF Run 句柄；`assistant_run_get` 仅可在同一进程内按显式 run ID 读取无正文的易失快照与安全事件，不支持省略 run ID 的活动 Run 查询、持久化断流回放或进程级恢复。完成正文只能由 `assistant_classified_run_take_result` 一次性取走。已持久化的涉密 Markdown 与会话数据继续构成 CEF 加密持久化边界，普通 SQLite 会话表不承载其正文。当前编辑器、活动 tab、scene、intent、旧 task ID 和笔记正文不进入隐式请求上下文；只有用户明确提交的引用和一次性 action snapshot 可以进入 Run。`Apply` 还必须把确认计划、模型工具参数和真实写入绑定到同一个显式目标与基准 hash；取消信号会进入 provider、工具调度和写盘前提交检查。

旧 `assistant_execute`、`ai_send_message`、`context_assemble`、`tool_confirm`、`session_*`、`agent_task_*`、`harness_*` 与独立领域执行入口均已移除。不会保留兼容 facade、第二状态机或双写。

## 搜索、联网与 Skills

普通搜索和 AI 检索均在 Rust 侧执行。Run 仅按显式引用和获授权范围请求材料；显式材料在读取/送模前、工具读取在打开文件前、Markdown 提交在写盘前都会复核文档策略。检索结果通过证据 ID 与安全展示元数据进入账本，不将证据正文作为系统指令。

模型请求只允许 HTTPS。联网开关是 `web.search` 的唯一授权源：关闭时 Native 与 MCP Web 调度均不可达，freshness、Skills、ChildRun 和提示词均不能增权。联网证据经 `WebEvidenceBroker`，仅接纳被显式映射为 `web.search`/`web.fetch` 且通过诊断的 provider。

通用 MCP 只开放另一条独立的 `external.read` 边界：`readOnlyHint=true` 只是服务端声明，不是 Iris 对第三方实现的证明；管理中心还会审查名称和递归输入 Schema，并要求用户对精确 provider/tool/schema 显式确认信任后才创建白名单 binding。Composer 必须为每个 normal-domain Run 显式选择 binding，Accept 事务会冻结用户信任位、binding hash、provider hash、transport/config、Schema、参数映射与输出策略。模型不能直接消费 discovery，也不能自行增权；classified、local-only、Skills 和隐式关键词均不能获得 `external.read`。运行中只执行冻结配置，并用 live provider hash/enablement 作撤销检查。输出仅接受最多 8,000 字符的文本或 JSON，证据摘录最多 2,000 字符；事件、审计和 checkpoint 不保存参数或原始输出。Iris 拒绝声明或 Schema 暴露写入、发送、删除、日历变更、进程和 secret 的工具，但无法独立验证已信任第三方服务端是否忠实实现其声明。Skills 是 prompt-only `SKILL.md`，不能安装外部包或执行代码。

## 凭据安全

API Key 使用本地 AES-256-GCM 加密存储，主密钥和密文分离；解密值由 `Zeroizing` 持有。日志、错误、事件、Run checkpoint 和诊断不包含 API Key、token、笔记正文或涉密路径。完整策略见 [SECURITY.md](./SECURITY.md)。

## CAS 版本快照加密

版本快照经内容寻址存储（`.iris/cas/`）并全程 AES-256-GCM 加密，永不落明文。加密密钥是**版本化密钥环**（`cas/encryption.rs`）：凭证记录保存 `{current, keys}` JSON，blob 头 `CAS2 + 版本号` 记录写入密钥版本，读取按版本取对应密钥；被轮换的旧密钥永久保留在环中，因此显式轮换（`rotate_cas_key`）永远不会让历史快照不可读。旧格式（纯 hex 单密钥 + `CASE` 头）在读取时按版本 0 兼容。

密钥获取 fail-hard：凭证记录存在但解密/解析失败时直接报错，**绝不静默生成新密钥覆盖**——静默轮换会让既有快照永久不可读（历史事故根因）。快照不可读（`AppError::CasUnreadable`，如磁盘损坏或密钥缺失）时系统自动降级：删除文档跳过不可读版本（manifest 标记 `unreadable`）、恢复跳过、`version_cleanup` 周期自动清理不可读版本行并释放 CAS 引用计数；用户全程无感知，事件仅出现在日志与既有健康检查。

## SQLite 与迁移

当前共有 **61 组**增量迁移（`001` 至 `061`）。

Schema 只允许通过带 up/down 的增量迁移变更。`051_agent_harness_cutover` 使用 copy-transform-swap 将旧会话、任务、trace 和审计外键迁移到统一 Run 模型；运行中或暂停的旧任务被安全归档为 `cancelled` 并带 `cancelled_legacy` 原因。迁移不要求用户删除数据库重建。

## IPC 契约

命令注册在 `src-tauri/src/lib.rs`，前端契约在 `src/types/ai.ts`、`src/types/ipc.ts` 与 `src/lib/ipc.ts`。修改 Tauri command 签名必须同步这些位置、相关测试和 [docs/ipc-api-reference.md](./docs/ipc-api-reference.md)。
