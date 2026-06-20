# Iris AI Agent 体系全面审查报告

> 审查模型：MIMO (xiaomi/mimo-v2.5-pro)
> 审查日期：2026-06-20
> 审查范围：长对话能力、长难任务深度思考解决能力、subagent 运行调度能力、工具调度能力、skills 安装及调度使用能力、网络检索能力、文件读写权限设置及读写能力、agent 权限设置安全性、沙箱设置及运行情况、agent 内部整体运行协作能力

---

## 一、总体架构评价

Iris 的 AI agent 体系是一个**设计精良、层次分明的系统**，包含约 80+ 个 Rust 源文件和 40+ 个前端组件，覆盖了从 LLM 网关到工具调度、权限控制、技能管理、检索增强的完整链路。代码质量整体较高，安全意识贯穿始终。

**架构评分：8/10**

---

## 二、逐模块分析

### 1. 长对话能力 — 6/10

**现状：**

- `session.rs:140-175` — `recent_messages()` 只取最近 N 条消息，无自动摘要
- `harness_support.rs:18-38` — `compress_history_messages()` 将超过 10 条的历史压缩为一行 `[历史摘要]`，每条仅取前 80 字符拼接
- `prompt_builder.rs:192` — 压缩后的 tool-role 消息被直接跳过
- `agent_task_policy.rs:260-273` — token 预算按意图硬编码（20K-240K），不与模型实际 context_window 关联

**问题：**

- **无对话摘要 LLM 调用**：压缩只是截断拼接，不是真正的语义摘要，50+ 轮对话后上下文质量严重下降
- **token 预算与模型脱节**：`agent_task_policy.rs:260` 硬编码 120K-240K 给 DocumentCheck，但用户可能配置 32K 窗口的模型，无防护
- **无显式 context window 截断**：`model_registry.rs:18` 的 `context_window` 字段存在但未被用于对话历史截断逻辑
- **`HISTORY_SUMMARY_THRESHOLD=10` 太低**：10 条就开始压缩，交互式对话很容易超过

### 2. 长难任务深度思考解决能力 — 7.5/10

**现状：**

- `agent_task.rs:62-81` — 8 状态 FSM（Queued→Running→{AwaitingConfirmation, PausedBudget, PausedRecoverable, Completed, FailedSafe, Aborted}）
- `agent_task.rs:649-664` — `build_budget_pause_checkpoint()` 支持预算耗尽时暂停并保存 continuation_goal
- `agent_task.rs:509-553` — `prepare_resume_plan()` + `validate_resume_preflight()` 支持 6 项恢复前检查
- `run.rs:977-1038` — `spawn_subagent` 支持子任务递归调用，深度限制为 2

**优点：**

- 检查点安全验证（`validate_checkpoint`, `agent_task.rs:1002`）阻止 21 类敏感键进入检查点
- 预检恢复验证检查 vault 范围、笔记路径、包可用性、技能、权限、模型槽位

**问题：**

- **无跨任务依赖追踪**：任务间完全独立，无法表达"B 依赖 A 的输出"
- **子任务预算分配固定**：`run.rs:1006` 固定 `parent_budget * 3/5`，不考虑任务复杂度
- **`AgentTaskStatus::parse` 静默降级**：`agent_task.rs:106-108` 未知状态默认为 `Running`，可能导致数据损坏

### 3. Subagent 运行调度能力 — 7/10

**现状：**

- `run.rs:420-452` — 主 harness 循环将 `spawn_subagent` 调用分区，递归执行
- `tool_policy.rs:124-126` — 深度 ≥ 2 时隐藏 `spawn_subagent`
- `run.rs:1009-1027` — 子任务继承父任务的 scene、session、skill_plan、task_policy

**问题：**

- **子任务无独立会话**：`run.rs:1012` 子任务共享父任务的 `session_id`，子任务的中间消息污染父对话
- **无并行子任务**：`run.rs:437-440` 使用 `futures` 顺序执行子任务，未利用并发
- **子任务失败传播不完整**：子任务失败只返回错误文本给父任务，无结构化错误码

### 4. 工具调度能力 — 8.5/10

**现状：**

- 87 个工具在 `tool_catalog` 注册，48 个有真实 dispatch handler
- 5 层过滤：Planned 硬门控 → 深度限制 → Web 开关 → 能力亲和 → 自治级别 → 确认要求
- 重试机制：`tool_dispatch_impl.rs:66-81` web 工具超时自动重试 1 次
- 回退机制：`search_hybrid` 失败回退到 `search_keyword`
- 10 个子模块分工明确：search、web、note、memory、schedule、vault、markdown、skills、boundary、runtime

**问题：**

- **`ToolRegistry::execute_tool()` 是存根**：`tool_executor.rs:104-136` 返回 `success: true` 和原始 args，实际 dispatch 在调用方完成，造成混淆
- **`DISPATCHABLE_TOOL_NAMES` 手动维护**：48 个工具名的列表需手动与 catalog 同步，虽然有测试守护但增加维护负担
- **`rendered_fetch` 名不副实**：`tool_dispatch/web.rs:64-71` 实际不做 JS 渲染，只输出警告
- **批量 web fetch 串行**：`tool_dispatch/web.rs:90` 5 个 URL 顺序获取，应并发

### 5. Skills 安装及调度使用能力 — 8/10

**现状：**

- 4 种安装源：URL、Git、本地文件、SkillHub 注册表
- BM25 + 向量重排序的激活评分系统（`activation.rs:154-339`）
- 每个 skill 有完整的生命周期：发现→安装→激活→注入→调度→诊断→卸载
- SHA-256 完整性校验、原子目录复制、符号链接逃逸防护

**问题：**

- **更新时无 SHA-256 校验**：`skill_install_service.rs:408-444` 的 `update_skill` 不传递 `expected_sha256`
- **SkillHub API 端点硬编码**：`skill_registry.rs:83` 硬编码 `https://api.skillhub.tencent.com`，违反 AGENTS.md 1.4 条款
- **评分阈值和权重是魔法数字**：0.35 阈值、3.0/4.0/2.5/2.0 权重均为经验调参，无正式评估
- **`scan_all_with_status` 性能**：每次 `list_skills` 都触发全目录扫描 + 能力预览计算

### 6. 网络检索能力 — 7.5/10

**现状：**

- 5 层混合检索：FTS5 + Vector(sqlite-vec) + Graph + Exact + Template
- `retrieval_broker_impl.rs:105-156` 各层错误静默吞没，保证容错
- 缓存层：`ContextAssemblyCache` + `PacketCache` 双层 LRU 缓存
- Web 工具需 `web_search_enabled` 标志 + L2 自治级别

**问题：**

- **检索层错误完全静默**：`retrieval_broker_impl.rs:113-156` 真实错误（如索引损坏、OOM）也会被忽略，无日志
- **`rendered_fetch` 无 JS 渲染能力**：只能获取静态 HTML，动态内容无法抓取
- **无网络代理配置**：所有 HTTPS 请求直连，无代理支持
- **`estimate_tokens` 极粗糙**：`context_planner.rs:429` 假设每 2 个中文字符 = 1 token

### 7. 文件读写权限设置及读写能力 — 8.5/10

**现状：**

- 文件系统沙箱：路径规范化 + 前缀检查 + 敏感路径黑名单 + 父目录组件拒绝
- 原子写入：`write_text_atomic` 先写 `.tmp` 再 rename
- 进程沙箱：仅允许 `wc`、`ls`、`rg`、`git`，`env_clear()`，5 秒超时
- Git 沙箱：vault 范围限定，硬编码 agent 身份

**问题：**

- **无 OS 级沙箱**：没有 seccomp/AppArmor/容器，完全依赖应用层逻辑
- **`rg` 参数限制可能不完整**：只阻止了 4 个 flag，未来新版本的 `rg` 可能增加写文件的 flag
- **`fs_export` 绝对路径处理**：`boundary.rs:98-99` 绝对路径直接使用，虽有 `starts_with` 检查但逻辑与相对路径分支不同

### 8. Agent 权限设置安全性 — 8/10

**现状：**

- 50+ 原子权限覆盖 vault、fs、doc、web、skills、process、git、clipboard、browser、secrets
- 4 级风险：Low → Medium → High → Critical
- `secret.read_plaintext` 永久标记为 `supported: false`（`agent_permissions.rs:426-428`）
- 敏感内容过滤：`validate_permission_storage_value` 阻止 api_key、token、password 等进入审计日志
- 审计链：每个权限决策记录到 `agent_permission_audit` 表

**问题：**

- **无工具调用速率限制**：agent 可以无限快速调用工具，无 per-request 或 per-session 限流
- **`META_SKILL_TOOLS` 绕过能力检查**：`tool_policy.rs:92-102` 的 9 个技能管理工具绕过任务能力和技能白名单，虽是设计意图但扩大了攻击面
- **隐私偏好硬编码**：`agent_task_policy.rs:131` 始终使用 `PrivacyPreference::ExternalAllowed`，不读取用户设置

### 9. 沙箱设置及运行情况 — 7/10

**现状：**

- 应用层沙箱覆盖文件系统、进程、网络、凭据四个维度
- `env_clear()` 防止环境变量泄露给子进程
- 凭据通过 OS Keychain 存储，内存中有 `zeroize()` 保护

**问题：**

- **无进程级隔离**：所有工具在同一个 Rust 进程内执行，一个工具的 panic 可能影响整个运行时
- **子进程无 seccomp**：`run_limited_process` 仅靠程序白名单 + 超时，被白名单的 `rg` 或 `git` 如果有漏洞可在 5 秒内执行任意操作
- **`env_clear()` 可能破坏 git**：git 需要 `HOME` 找配置，清除后可能使用默认值或失败
- **Circuit breaker 全局可变状态**：`circuit_breaker.rs:29-30` 使用 `LazyLock<Mutex<HashMap>>`，poisoned mutex 时静默恢复可能掩盖并发 bug

### 10. Agent 内部整体运行协作能力 — 7.5/10

**现状：**

- 清晰的分层：Policy → Context Planner → Retrieval Broker → Packet Builder → Model Gateway → Harness
- 消息修复管线：3-pass 修复确保 tool-call 协议合规
- Prompt 构建 7 层缓存友好布局
- 前端 11 个 hooks 分工明确

**问题：**

- **前端 hook 参数爆炸**：`useAssistantTasks` 接受 55+ 参数，`send()` 有 27 个依赖项
- **隐式 ref 耦合**：多个 `MutableRefObject` 在 hooks 间传递，依赖关系难以追踪
- **Agent 任务状态无实时更新**：`useAgentTaskStatus` 只在 mount 时获取一次，运行中的任务显示过时数据
- **Run plan 状态只写不读**：`useAssistantRunPlan` 存储的状态从未被渲染

---

## 三、关键 Bug

| 严重度 | 位置 | 描述 |
|--------|------|------|
| **Critical** | `useAssistantConfirmations.ts:245` | **乱码中文**：`宸ュ叿纭澶辫触` 应为 `工具确认失败`，编码损坏导致用户看到乱码 |
| **Medium** | `streaming.rs:56-64` | 流式请求硬编码 OpenAI 格式，Anthropic 流式会静默失败 |
| **Medium** | `model_gateway_impl.rs:476` | Anthropic 非流式响应的 tool_calls 永远返回空 Vec |
| **Medium** | `agent_task.rs:106-108` | `AgentTaskStatus::parse` 未知状态静默降级为 `Running` |
| **Low** | `streaming.rs:108-118` | SSE carry buffer 超 1MiB 时静默截断，仅 warn 日志 |
| **Low** | `agent_task_policy.rs:260` | token 预算与模型实际 context_window 脱节 |
| **Low** | `retrieval_broker_impl.rs:113-156` | 检索层真实错误被静默吞没 |

---

## 四、安全漏洞与隐患

| 风险 | 位置 | 描述 |
|------|------|------|
| **中** | 全局 | 无工具调用速率限制，agent 可快速循环调用 |
| **中** | `skill_registry.rs:83` | SkillHub API 端点硬编码，违反 AGENTS.md 1.4 |
| **低** | `tool_policy.rs:131` | 隐私偏好硬编码为 `ExternalAllowed` |
| **低** | `boundary.rs:489` | `env_clear()` 可能破坏需要 HOME 的子进程 |
| **低** | `agent_task_policy.rs:106` | 未知状态降级可能被利用绕过状态检查 |
| **信息** | `guardrails.rs:76-305` | 注入检测基于关键词模式，可被同形字变体或编码绕过（已有同形字归一化但不完整） |

---

## 五、设计亮点

1. **检查点安全验证**（`agent_task.rs:1002`）：21 类敏感键的递归检查 + 大小限制
2. **多层权限防护**：ToolPolicy → PermissionAtoms → DispatchBoundary → Guardrails → CircuitBreaker → Audit
3. **凭据零知识**：API key 仅存 OS Keychain，内存 `zeroize()`，capability 只检查 boolean marker
4. **源码级契约测试**：前端测试直接读源码字符串验证架构不变量
5. **消息修复管线**：3-pass 修复处理遗留数据、孤儿消息、缺失 stub
6. **BM25 + 向量混合排序**：技能激活兼顾关键词和语义匹配
7. **原子文件操作**：`atomic_copy_dir` + `write_text_atomic` 防止半写
8. **敏感内容过滤**：审计日志中 11 类敏感模式被拦截

---

## 六、改进建议优先级

| 优先级 | 建议 |
|--------|------|
| **P0** | 修复 `useAssistantConfirmations.ts:245` 乱码 bug |
| **P0** | 补全 Anthropic 流式 + tool_calls 解析支持 |
| **P1** | 为 token 预算添加模型 context_window 上限校验 |
| **P1** | 为工具调用添加 per-session 速率限制 |
| **P1** | `AgentTaskStatus::parse` 未知值应返回错误而非静默降级 |
| **P2** | 添加对话摘要 LLM 调用替代简单截断 |
| **P2** | 检索层错误区分"表不存在"和"真实错误"，后者应记录日志 |
| **P2** | 移除 SkillHub 硬编码端点，改为可配置 |
| **P3** | `web_fetch_batch` 改为并发获取 |
| **P3** | 前端 hooks 参数重构，减少 prop drilling |

---

## 七、防御层次总览

| 层次 | 组件 | 防护目标 |
|------|------|----------|
| L1 | ToolPolicy (`tool_policy.rs`) | 未实现工具暴露、自治级别违规、能力不匹配 |
| L2 | PermissionAtoms (`agent_permissions.rs`) | 缺少权限授予、敏感数据进入审计日志 |
| L3 | DispatchBoundary (`tool_dispatch/boundary.rs`) | 路径遍历、系统目录访问、进程白名单、环境变量泄露 |
| L4 | Guardrails (`guardrails.rs`) | 提示注入、同形字绕过、零宽字符绕过、引用伪造 |
| L5 | CircuitBreaker (`circuit_breaker.rs`) | LLM 提供商级联故障 |
| L6 | AuditTrail (`tool_audit.rs` + `trace.rs`) | 所有权限决策的事后取证分析 |

---

_报告由 MIMO (xiaomi/mimo-v2.5-pro) 生成，基于对 80+ Rust 源文件和 40+ 前端组件的逐一审查。_
