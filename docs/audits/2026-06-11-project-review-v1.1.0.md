# 2026-06-11 项目审查与 v1.1.0 稳健修复

本文记录 v1.1.0 统一版本与全项目低风险审查跟进。当前事实来源仍为 README、ROADMAP、ARCHITECTURE、AGENTS、docs/README 与当前代码；`docs/history/`、`docs/plans/`、`docs/superpowers/` 属于历史或施工资料，引用前必须核对代码实现。

## 已修复

- 版本事实源统一为 `1.1.0` / `v1.1.0`：npm、Cargo、Tauri manifest/lock、设置页“关于 Iris”、README、ROADMAP、CHANGELOG、docs 索引与语义评测文档已同步。
- Web fetch/search User-Agent 已更新为 `Iris/1.1.0`。
- Tauri CSP `connect-src` 补齐 `https://api.deepseek.com`，与默认 DeepSeek 路由、设置页提示、ARCHITECTURE 与 `docs/llm-routing.md` 一致。
- 删除已被 git 跟踪的 `.superpowers/brainstorm/**` 本地过程产物；`.gitignore` 已覆盖后续同类文件。
- 删除无项目引用的 `.brooks-lint-history.json` 本地审查历史。
- 删除已被 Ghost Spine 取代的 `docs/plans/2026-06-10-outline-luminous-rail-design.md`。

## 审查结论

### 代码质量

- 前端 IPC 调用仍集中在 `src/lib/ipc.ts`，未发现业务组件直接绕过封装调用 `invoke()`。
- TypeScript 未发现源代码级 `any` 类型写法；测试中的 `expect.any` 属 Vitest matcher，不属于类型逃逸。
- Rust `unsafe` 仅见于 `sqlite-vec` feature 下的 `sqlite3_auto_extension` 注册路径，带 SAFETY 注释；后续若 PR 启用该 feature，仍需在 PR 描述中显式标注。

### 功能正确性

- DeepSeek 是当前默认 LLM 路由；CSP 缺少 DeepSeek 域名会导致运行契约与文档/设置页不一致，已修复。
- `sqlite-vec` 继续保持 optional/experimental；默认语义检索依赖 Rust cosine fallback 与 scalar 知识表，不把 Windows feature 构建作为当前质量门禁。

### 性能与运行开销

- `Database` 维持 WAL、读写连接池和 `mmap_size` 配置；本轮不调整数据库策略。
- `UnifiedAssistantPanel`、`App.tsx`、`ai_runtime/skills.rs`、`model_gateway.rs` 仍是后续拆分候选。拆分应作为行为保持型重构单独推进，并先补聚焦测试。
- 10000+ 笔记冷启动、内存占用和 Playwright 全链路仍是延后质量目标，未在本轮声称达标。

### 架构与边界

- 未新增 IPC、数据库 migration、公共类型或依赖。
- 文档清理采用保守归档策略：保留根本文档和专题事实源，删除明确废弃且无独立证据价值的短文档。

### 安全与合规

- API Key 存储路径仍为 OS 凭据管理器；未新增明文凭据落盘路径。
- HTTPS-only/rustls 客户端保持不变；本轮仅补齐允许连接的 DeepSeek 官方域名。
- 依赖许可未变，项目包元数据继续声明 `AGPL-3.0-only`。

## 后续优先级

1. 将 `UnifiedAssistantPanel` 拆为消息流、工具确认、上下文抽屉和运行状态子模块。
2. 将 `App.tsx` 中标签页、外部文件同步、覆盖层调度继续下沉到 hooks/model。
3. 给 DeepSeek/CSP 契约补源代码测试，避免后续新增默认 provider 时遗漏 Tauri CSP。
4. 为 10000+ 笔记目录建立可重复性能基准，避免 ROADMAP 指标长期停留在文字状态。
