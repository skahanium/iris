# Iris 架构

> 本文描述当前实现边界，不承担版本承诺；版本排期只在 [ROADMAP.md](./ROADMAP.md) 维护。

## 分层

```text
React 19 UI
  └─ src/lib/ipc.ts（类型安全 IPC 封装）
       └─ Tauri commands
            ├─ 文件、索引、搜索、版本与回收站
            ├─ LLM 配置、模型网关与 AI Runtime
            └─ SQLite / 本地加密凭据 / Vault 文件系统
```

- `src/`：React、TipTap、状态和类型安全 IPC 调用；前端不直接调用 `invoke()`。
- `src-tauri/src/commands/`：Tauri 命令边界和输入校验。
- `src-tauri/src/ai_runtime/`：任务规划、检索 broker、证据包、模型网关、工具确认与追踪。
- `src-tauri/src/indexer/`：Markdown/frontmatter、分块、链接、标签和索引更新。
- `src-tauri/src/storage/`：SQLite、迁移、FTS 与可选 sqlite-vec 注册。

## 数据原则

用户 `.md` 是笔记唯一权威来源。`files`、`chunks`、`links`、FTS 和嵌入索引均可由 Vault 重建；会话、网页缓存和收件箱等属于应用状态。应用不在未经确认时改写用户笔记。

## 搜索与 AI 上下文

普通搜索和 AI 检索都在 Rust 侧执行。当前索引包含 FTS、`chunk_embeddings`、显式链接、标签、语义锚点与法规索引；AI Runtime 通过 retrieval broker 合并候选并构建携带来源、span、hash、分数与信任级别的 `ContextPacket`。

sqlite-vec 是可选加速能力，默认构建可在不可用时保留非向量检索。v1.2.6 将统一向量模型、索引代际和 broker 降级语义；具体设计见 [RAG 优化设计](./docs/specs/v1.2.6-rag-optimization.md)。

## AI、网络与 Skills

模型请求只允许 HTTPS。LLM 路由、模型目录和能力选择的实现说明见 [docs/llm-routing.md](./docs/llm-routing.md)。联网证据经过 `WebEvidenceBroker`，只接纳被显式映射为 `web.search` / `web.fetch` 的 provider；普通证据详情不暴露密钥或原始敏感载荷。

Skills 仅是 prompt-only `SKILL.md`；由 Iris 创建草稿、用户确认内容和范围后启用，不能安装外部包或执行代码。

## 凭据安全

API Key 使用本地 AES-256-GCM 加密存储，不使用系统 Credential Manager。主密钥在平台配置目录，密文在应用数据目录；每条记录使用随机 12 字节 nonce 和服务名 AAD，解密值由 `Zeroizing` 持有。完整安全策略见 [SECURITY.md](./SECURITY.md)。

## SQLite 与迁移

截至当前基线共有 **43 组**增量迁移（`001` 至 `043`），每组都含 up/down 脚本。schema 变更只能新增迁移，不能要求用户删除数据库重建。依赖 sqlite-vec 的历史迁移按扩展可用性 best-effort 执行；标量表的可用性不依赖该扩展。

## IPC 契约

命令注册在 `src-tauri/src/lib.rs`；前端契约在 `src/types/ipc.ts` 和 `src/lib/ipc.ts`。任何 Tauri 命令签名变化必须三处同步，并更新 [docs/ipc-api-reference.md](./docs/ipc-api-reference.md)。
