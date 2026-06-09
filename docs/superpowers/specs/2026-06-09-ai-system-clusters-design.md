---
title: AI 体系五集群修复设计
created: 2026-06-09
revision: 2026-06-09-2
scope: ai_harness, ai_runtime, tool_dispatch, tool_policy, tool_catalog, skills, persona, prompt_profile, ToolConfirmDialog, PatchPreview
---

# AI 体系五集群修复设计

## 摘要

基于对 Iris AI 子系统的全面审计，将问题归为五个独立集群：

- **A. 安全与正确性**（3 项）：git hook 风险、工具无超时、parse retry 无限循环
- **B. 工具体系卫生**（3 项）：双轨权限收敛、execute_tool 删除、DISPATCHABLE_TOOL_NAMES 消重
- **C. Skills 激活体系**（3 项）：BM25 + embedding 匹配、匹配触发模式、资源长度限制
- **D. 编辑确认体验**（1 项）：写入确认接入 diff 预览
- **E. 性能与工程卫生**（7 项）：preset display_name、重复人设注入、ToolPolicy 缓存、SQLite spawn_blocking、并行工具执行、fastembed 预热、场景预算翻倍

所有改动不改变外部行为契约，不新增依赖（C 集群复用已有 fastembed 依赖）。

---

## Cluster A：安全与正确性

### A1. install_from_git Git Hook 禁用

**改动文件**：`src-tauri/src/ai_runtime/skills.rs`（`install_from_git` 函数）

- `git clone` 参数从 `["clone", "--depth", "1", "--"]` 改为 `["clone", "--depth", "1", "-c", "core.hooksPath=NUL", "--"]`
- clone 完成后用 `load_skill` 预读验证 SKILL.md 合法性，不合规则清理 tmp 并返回错误
- tmp 目录使用 drop guard 确保异常路径也清理

### A2. 工具执行超时

**改动文件**：`src-tauri/src/ai_runtime/tool_dispatch.rs`（`dispatch_tool_with_retry`）

- 新增超时常量：网络 30s、检索 10s、文件 5s
- 用 `tokio::time::timeout` 包裹，超时返回 `ToolCallResult { success: false, error: "tool timed out" }`
- retry 逻辑仅在非 timeout 的 transient error 时触发

### A3. Parse Retry 上限

**改动文件**：`src-tauri/src/ai_harness/harness/run.rs`

- 声明 `parse_retries` 计数器，上限 3 次
- 超限后放弃工具解析，把 content 当最终回答返回

---

## Cluster B：工具体系卫生

### B1. 双轨权限收敛

**改动文件**：`src-tauri/src/ai_workflows/research_workflow.rs`、`src-tauri/src/ai_runtime/tool_executor.rs`

- `research_workflow.rs:293-299` 迁移到 `registry.check_tool_policy`
- `check_tool_permission` 标注 `#[deprecated]`

### B2. ToolRegistry::execute_tool 删除

**改动文件**：`src-tauri/src/ai_runtime/tool_executor.rs`

- 删除 `execute_tool` 方法及其测试。零调用者。

### B3. DISPATCHABLE_TOOL_NAMES 消重

**改动文件**：`src-tauri/src/ai_runtime/tool_dispatch.rs`、`src-tauri/src/ai_runtime/tool_catalog.rs`

- 删除 `tool_dispatch.rs` 中两个常量，改为引用 `catalog_dispatchable_names()` / `catalog_harness_only_names()`

---

## Cluster C：Skills 激活体系

### C1. 技能匹配引擎

**数据流**：安装/扫描时 fastembed 计算 embedding 写入 `skill_activation_index` 和 vec 虚拟表；harness 启动时 BM25 粗筛 top-20 → embedding 重排 → top-3（score >= 0.35）。

**回退链**：sqlite-vec 可用 → vec 表；不可用 → embedding_json + Rust cosine；fastembed 失败 → 纯 BM25 top-5。

**改动**：`skills_for_scene` 替换为新匹配逻辑；`active_skill_allowed_tools` 自动继承；`prepare_environment_and_skills` 注入 top-3 skill 的 content。

### C2. 匹配触发模式

A + C 混合：harness 启动时自动匹配 top-3；用户可显式从 UI 覆盖。不逐轮重匹配。

### C3. skills_read_resource 长度限制

`MAX_SKILL_RESOURCE_CHARS = 24_000`，超长截断并标记 `truncated: true`。

---

## Cluster D：编辑确认体验

### D1. 写入确认接入 Diff 预览

**核心决策**：后端计算上下文（读磁盘文件），前端渲染（DiffView）。

**后端**（`pause_for_tool_confirmation`）：检测写入工具后读文件，计算 `before_context` / `after_context`，写入 `confirm_request.preview`。

**前端**：从 `PatchPreview.tsx` 抽取 `DiffView` 子组件（read-only），在 `ToolConfirmDialog.tsx` 的 `showPatchReview` 分支中渲染。

---

## Cluster E：性能与工程卫生

### E1. Persona 预设 display_name

三个预设的 `display_name` 改为 `"学者"` / `"墨韵"` / `"疾风"`。

### E2. 重复人设注入

删除 `environment.rs` 中 `to_system_prompt_fragment()` 调用，人设注入仅走 `PersonaResolver`。

### E3. ToolPolicy 逐轮缓存

harness 循环开始处调用 `compute_available_tools` 一次，同轮后续 O(1) 集合查找。

### E4. SQLite spawn_blocking

所有 `state.db.with_read_conn` 用 `tokio::task::spawn_blocking` 包裹，优先覆盖检索类工具。

### E5. 并行工具执行

同轮只读无副作用工具用 `join_all` 并发执行，统一收集后推 tool messages。

### E6. Fastembed 预热

应用启动后异步调用 `embed_text("warmup")`，失败静默忽略。

### E7. 场景预算翻倍

**改动文件**：`src-tauri/src/ai_types/mod.rs`（`resolve_scene`）

| 场景              | 默认    | 上限    |
| ----------------- | ------- | ------- |
| KnowledgeLookup   | 100,000 | 240,000 |
| ExemplarLearning  | 120,000 | 320,000 |
| DraftingAssist    | 160,000 | 320,000 |
| ResearchSynthesis | 200,000 | 480,000 |

---

## 测试计划

- **A1**：`install_from_git_rejects_invalid_skill`
- **A2**：`test_timeout_returns_error`
- **A3**：`parse_retry_stops_after_3`
- **C1**：`skill_matching_bm25_precision`、`skill_matching_embedding_rerank`
- **C3**：`read_skill_resource_truncates_over_limit`
- **E3**：`policy_cache_consistent_with_individual_eval`

回归：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`npm run lint`、`npm run format:check`、`npm run typecheck`

---

## 假设

1. sqlite-vec feature 默认开启；fastembed 模型已预下载
2. `environment.rs` 中 `to_system_prompt_fragment` 删除不影响其他 workflow
3. 并行工具执行中结果顺序与串行时一致
4. Cluster C 的 BM25 复用项目中已有的 `bm25_score` 函数
5. 父子 Agent 预算各自独立，维持当前 60% 子预算分配逻辑
