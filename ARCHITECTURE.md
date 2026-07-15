# Iris 架构

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

## 数据原则

用户 `.md` 是笔记唯一权威来源。`files`、`chunks`、`links`、FTS 与嵌入索引均可由 Vault 重建；会话、Run、网页缓存和收件箱属于应用状态。应用不会在未确认时改写用户笔记。

## Agent Run

`assistant_run_start`、`assistant_run_control` 和 `assistant_run_get` 是唯一执行、控制和恢复入口。每个 Run 先持久化 accepted，再进行策略、上下文、路由与 provider 调度；`assistant:run_event` 是唯一的前端生命周期事件，断流使用 `assistant_run_get` 回放。

会话通过不透明 `AssistantSessionRef` 寻址，并按 normal/classified 安全域物理隔离。当前编辑器、活动 tab、scene、intent、旧 task ID 和笔记正文不进入隐式请求上下文；只有用户明确提交的引用和一次性 action snapshot 可以进入 Run。

旧 `assistant_execute`、`ai_send_message`、`context_assemble`、`tool_confirm`、`session_*`、`agent_task_*`、`harness_*` 与独立领域执行入口均已移除。不会保留兼容 facade、第二状态机或双写。

## 搜索、联网与 Skills

普通搜索和 AI 检索均在 Rust 侧执行。Run 仅按显式引用和获授权范围请求材料；检索结果通过证据 ID 与安全展示元数据进入账本，不将证据正文作为系统指令。

模型请求只允许 HTTPS。联网证据经 `WebEvidenceBroker`，仅接纳被显式映射为 `web.search`/`web.fetch` 且通过诊断的 provider。Skills 是 prompt-only `SKILL.md`，不能安装外部包或执行代码。

## 凭据安全

API Key 使用本地 AES-256-GCM 加密存储，主密钥和密文分离；解密值由 `Zeroizing` 持有。日志、错误、事件、Run checkpoint 和诊断不包含 API Key、token、笔记正文或涉密路径。完整策略见 [SECURITY.md](./SECURITY.md)。

## SQLite 与迁移

当前共有 **54 组**增量迁移（`001` 至 `054`）。

Schema 只允许通过带 up/down 的增量迁移变更。`051_agent_harness_cutover` 使用 copy-transform-swap 将旧会话、任务、trace 和审计外键迁移到统一 Run 模型；运行中或暂停的旧任务被安全归档为 `cancelled` 并带 `cancelled_legacy` 原因。迁移不要求用户删除数据库重建。

## IPC 契约

命令注册在 `src-tauri/src/lib.rs`，前端契约在 `src/types/ai.ts`、`src/types/ipc.ts` 与 `src/lib/ipc.ts`。修改 Tauri command 签名必须同步这些位置、相关测试和 [docs/ipc-api-reference.md](./docs/ipc-api-reference.md)。
