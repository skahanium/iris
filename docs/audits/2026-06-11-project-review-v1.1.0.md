# 2026-06-11 项目审查与 v1.1.0 稳健修复

本文记录 v1.1.0 统一版本与全项目低风险审查跟进。当前事实来源仍为 README、ROADMAP、ARCHITECTURE、AGENTS、docs/README 与当前代码；`docs/history/`、`docs/superpowers/` 属于历史或施工资料，引用前必须核对代码实现。

## 已修复

- 版本事实源统一为 `1.1.0` / `v1.1.0`：npm、Cargo、Tauri manifest/lock、设置页“关于 Iris”、README、ROADMAP、CHANGELOG、docs 索引与语义评测文档已同步。
- Web fetch/search User-Agent 已更新为 `Iris/1.1.0`。
- Tauri CSP `connect-src` 补齐 `https://api.deepseek.com`，与默认 DeepSeek 路由、设置页提示、ARCHITECTURE 与 `docs/llm-routing.md` 一致。
- 删除已被 git 跟踪的 `.superpowers/brainstorm/**` 本地过程产物；`.gitignore` 已覆盖后续同类文件。
- 删除无项目引用的 `.brooks-lint-history.json` 本地审查历史。
- 删除已被 Ghost Spine 取代的 `docs/plans/2026-06-10-outline-luminous-rail-design.md`。
- 将 `docs/plans/` 阶段性施工文档整体归档到 `docs/history/plans/`；将旧 Notion 参考摘要移至 `docs/history/2026-06-11-notion-reference-summary.md`。

## 审查结论

### 发现与修复验收

1. **已修复：AI 面板 token usage 重复逻辑收敛。** `setSessionTokenUsage` 的三处手写累加已替换为 `src/lib/token-usage.ts` 的 `accumulateTokenUsage`，并由 `tests/token-usage.test.ts` 覆盖空会话、已有会话与可选 cache counter 场景。`UnifiedAssistantPanel` 仍是大组件，但本轮已消除最容易漏改的统计重复点。
2. **已修复：应用壳编辑器统计状态下沉。** `src/App.tsx` 不再内联维护 `editorStatsTimerRef`，改用 `src/hooks/useEditorStats.ts`；`StatusBarContext` 也复用同一 hook，减少两处 debounce 实现分叉。
3. **已修复：编辑器统计 debounce 定时器补齐卸载清理。** `useEditorStats` 在 unmount 时 `clearTimeout(editorStatsTimerRef.current)`，并由 `tests/runtime-contracts.test.ts` 做源代码契约保护。
4. **已修复：Skills 注入路径增加提示词体积上限。** `inject_into_prompt` 通过 `MAX_SKILL_PROMPT_BODY_CHARS` 截断过长 skill 正文，并在截断处写入说明；`cargo test inject_into_prompt_truncates_large_skill_body` 覆盖长正文不会完整进入 prompt。
5. **已修复：DeepSeek/CSP 运行契约有自动测试。** `tests/runtime-contracts.test.ts` 读取 Tauri CSP、Rust provider host 和默认路由，防止后续默认 DeepSeek 路由与 CSP 再次漂移。
6. **已修复：`sqlite-vec` unsafe feature 增加 manifest 级治理。** `sqlite-vec` 继续 optional 且默认关闭；`src-tauri/Cargo.toml` 明确标注其注册路径使用 unsafe、启用 release build 前需要 focused maintainer review，并由 runtime contract test 保护。
7. **已修复：涉密密码设置失败后清空敏感输入。** `ClassifiedPasswordSetup` 的 IPC 抛错路径现在会清空密码和确认值；`tests/classified-password-setup.test.tsx` 覆盖失败路径。
8. **已修复：历史设计文档完成二次归档。** `docs/plans/` 阶段性施工文档已移入 `docs/history/plans/`；旧 Notion 参考摘要已移入 `docs/history/2026-06-11-notion-reference-summary.md`，当前索引不再把它们列为事实源。

### 代码质量

- 前端 IPC 调用仍集中在 `src/lib/ipc.ts`，未发现业务组件直接绕过封装调用 `invoke()`。
- TypeScript 未发现源代码级 `any` 类型写法；测试中的 `expect.any` 属 Vitest matcher，不属于类型逃逸。
- Rust `unsafe` 仅见于 `sqlite-vec` feature 下的 `sqlite3_auto_extension` 注册路径，带 SAFETY 注释；该 feature 默认关闭，并在 Cargo manifest 中标注 focused maintainer review 要求。

### 功能正确性

- DeepSeek 是当前默认 LLM 路由；CSP 缺少 DeepSeek 域名会导致运行契约与文档/设置页不一致，已修复。
- `sqlite-vec` 继续保持 optional/experimental；默认语义检索依赖 Rust cosine fallback 与 scalar 知识表，不把 Windows feature 构建作为当前质量门禁。

### 性能与运行开销

- `Database` 维持 WAL、读写连接池和 `mmap_size` 配置；本轮不调整数据库策略。
- `UnifiedAssistantPanel`、`App.tsx`、`ai_runtime/skills.rs`、`model_gateway.rs` 仍可继续做更深拆分；本轮已完成 token usage、editor stats 和 skill prompt 体积上限这三处可验证降耦/降开销修复。
- 10000+ 笔记冷启动、内存占用和 Playwright 全链路仍是延后质量目标，未在本轮声称达标。

### 架构与边界

- 未新增 IPC、数据库 migration、公共类型或依赖。
- 文档清理采用保守归档策略：保留根本文档和专题事实源，删除明确废弃且无独立证据价值的短文档。

### 安全与合规

- API Key 存储路径仍为 OS 凭据管理器；未新增明文凭据落盘路径。
- HTTPS-only/rustls 客户端保持不变；本轮仅补齐允许连接的 DeepSeek 官方域名。
- 依赖许可未变，项目包元数据继续声明 `AGPL-3.0-only`。

## 后续优先级

1. 将 `UnifiedAssistantPanel` 的消息流、工具确认、上下文抽屉和运行状态继续拆为更小子模块。
2. 将 `App.tsx` 中标签页、外部文件同步、覆盖层调度继续下沉到 hooks/model。
3. 为 10000+ 笔记目录建立可重复性能基准，避免 ROADMAP 指标长期停留在文字状态。

## 2026-06-11 深拆与性能重构跟进

### 已落地

- 前端入口瘦身：`src/components/ai/UnifiedAssistantPanel.tsx` 与 `src/App.tsx` 已改为薄 facade，原实现迁到 `*.impl.tsx`，并用模块边界契约测试锁定入口行数。
- AI 面板普通编辑性能：`UnifiedAssistantPanel` 不再接收整篇 `noteContent` 字符串，改为 `getNoteContent: () => string`；章节/文档任务执行时才解析正文，避免普通编辑触发 `parseDocumentChapters`。
- 性能/契约测试：新增 AI 面板性能契约、模块边界契约和 context cache Rust 测试；扩展 `ai_benchmarks.rs` 覆盖 skill prompt 注入、长 tool history body 构造、retrieval query hash 与 guardrail 大文本扫描。
- Rust runtime 边界：`model_gateway`、`skills`、`tool_dispatch`、`tool_catalog`、`retrieval_broker` 保留旧 public import path，通过 facade 指向 `*_impl.rs`。
- 上下文组装缓存：新增 `ai_runtime::context_cache`，`context_assemble` 与 `ai_send_message` 复用同一路径；`ai_cache_clear`、runtime clear、knowledge reindex 会失效缓存。

### 仍需后续继续

- 当前 `*.impl.tsx` / `*_impl.rs` 仍承载原大实现，入口已经变薄，但还没有完全拆成最终 hooks 和按领域 Rust 子模块。
- `ConversationSurface/AiMessageList` 虚拟化、artifact 与流式 token 状态完全隔离，仍需在下一阶段以渲染回归测试驱动。
- 10000+ vault 的跨机器绝对耗时目标仍未建立稳定门槛；当前只补了可重复 benchmark 覆盖面。
