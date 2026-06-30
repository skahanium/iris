# Iris Skills / MCP / 网络能力收口设计

日期：2026-07-01

## 摘要

Iris 是桌面端、单用户、本地优先的 AI 原生 Markdown 笔记软件。在本设计之前，Iris 在 Agent 建设上走过一段弯路：参照通用 AI Agent（Hermes、OpenClaw、Claude Code 等）的规格建设了 Skills 体系与 MCP 体系，把 Iris 当作通用 Agent 平台来搭。这一取向偏离了 Iris 的核心——笔记本身。

本设计把过度生长的能力收口回笔记中心，分三块交付：

- **块 A — Skills 收口为自产 prompt-only**：Skills 不再是"受控技能能力平台"，回到 ROADMAP 原定义的"用户安装的 `SKILL.md` 提示词包"，并进一步收窄为"Iris 内部对话驱动、自产自销的纯 prompt 包"。Skill 永不连 MCP、不执行脚本、不安装依赖、不直接写文件；动 vault 一律经 `PatchProposal` + scope gate + 用户确认。
- **块 B — MCP runtime 限用**：已建成的 `McpHostRuntime`（stdio + HTTPS transport + registry + health + inventory）保留不拆；仅在"适用范围"上收口——MCP 不再对 agent 暴露任意 tool，仅作为 `WebEvidenceBroker` 后端的 web search/fetch provider；`skill.mcp_bridge` 这条"skill 作为 MCP capability 消费者"的虚拟身份彻底清除。
- **块 C — 网络能力与 AI 联网生命周期收口**：把碎片化的 5 处直接 `search_web` 调用 + 退役的 prompt-prefix 注入路径 + 假实现的 `rendered_fetch` 全部收敛进 `WebEvidenceBroker` 单一入口；按《Iris 网络能力、MCP 地基与 AI 联网生命周期设计》(2026-06-21) 的主干补齐 Policy Gate / Query Plan / Provider 调度 / 结果归一化 / 缓存隔离 / 证据注入边界 / 引用摘要 / 审计 / 留存；同时削掉该文档中与"笔记核心 + MCP provider 即强搜索主力"哲学冲突的过度部分（7-provider native pool、垂直学术源、三档预算竞速、持续收集模式）。

self-improvement / Proactive Agent 这类被 Skills 路径错误承担的能力，本设计明确改由 harness 原生内部集成；该项目单独立项，不在本 spec 范围。

## 目标

- Skills 回归"纯 prompt 包"，由 Iris 对话内置 skill-creator 生成；权限天花板是对 vault 中**用户批准范围**内文档的"提出写入建议（PatchProposal）"，激活无需确认、动 vault 时需用户确认。
- MCP runtime 已建成能力不拆，仅收口适用范围：web.search / web.fetch 经 broker，不再对 agent 暴露任意 MCP tool。
- 整个网络能力层（搜索 + 抓取）通过单一 `WebEvidenceBroker` 入口，受底部栏联网开关统一闸控；native（MiniMax / DDG）与 MCP 搜索 provider 协调调度，每次搜索可"一主一补"并发。
- 网页抓取由 broker 统管（搜索后自动 enrich top-K 结果页），不再作为独立 agent tool。
- 补齐证据归一化字段、缓存隔离、证据注入边界、审计脱敏、留存清理——遵循 2026-06-21 文档主干。
- 消除 `skill.mcp_bridge` 在 capability_resolver / agent_permissions / skill_trust_policy / skills/compatibility 四层之间"supported / Critical / high_risk / Planned"的矛盾信号。

## 非目标

- 不砍 `McpHostRuntime` 的 stdio / HTTPS transport、registry、health、inventory 等已建成能力。
- 不删 `AgentPermissionAtom` 中的 `Process*` / `Secret*` / `SkillMcpBridge` 等词汇本身——仅停用其解析入口与通过 agent tool 暴露的路径。
- 不自建 7-provider native 强搜索池、垂直学术源（OpenAlex / PubMed / GDELT / GitHub / RSS）、三档预算竞速、research 持续收集模式——这些改为"用户自配 MCP server"路径，不在本 spec 实现。
- 不承诺 MCP provider 自动授予能力。MCP server 自报 annotations 不产生权限，仍由 Iris registry 与 broker 决定。
- 不在本次实现 self-improvement / Proactive Agent；它们是后续独立 spec。
- 不把 Skills 做成插件运行时——不能扩展 UI、不能注册新节点类型、不能执行任意代码、不能连 MCP。

## 前因后果

### 弯路是怎样形成的

Iris 在 v0.5.x 的 AI 建设中，Skills 与 MCP 两条线几乎都参照通用 AI Agent 的规格推进：

- Skills 体系演化出 6 种 kind（`legacy_prompt_only` / `prompt_only` / `resource` / `workspace` / `mcp_dependent` / `hybrid`），带上 trust profile 风险分级、workspace 层、capability 消费者身份、`mcp.dependencies` 声明、closed-loop diagnostics，以及给模型暴露的 `skills_install` / `skills_write` / `skills_read` 等管理工具——已经是一个"受控技能能力平台"，超出 ROADMAP 给出的"用户安装 `SKILL.md` 提示词包"原定义。
- MCP 体系建成了完整的 `McpHostRuntime`（2064 行）、registry（38KB / 4 张表 / 11 个 IPC），但**周围**又长出了与笔记无关的一般 Agent 能力：`capability_resolver` 接受 `process.run_readonly` / `secret.use_named` / `skill.mcp_bridge` 等 13 条 capability，`agent_permissions` 配套 30+ 个 permission atom，`sandbox_profile` 路由到未实现的 L2 OS 边界。
- 网络能力的《Iris 网络能力、MCP 地基与 AI 联网生命周期设计》(2026-06-21) 把上面这套"通用研究 Agent"的雄心写到了极致：7 个 native provider 适配、垂直学术源、quick/standard/research 三档预算、provider 竞速、持续收集——这是把 Iris 当作通用深度研究 Agent 来设计的关键拐点。

### 为什么这是弯路

- **本质错位**：Iris 的核心是 Markdown 笔记。AI 能力应围绕"读笔记、改笔记、给笔记加证据、和笔记对话"展开，而不是变成一个能跑任意 MCP server、能调任意 process tool、能持续爬网的通用 Agent。
- **市场上没有适配的 skills**：当前网络上流行的 skills 基本是 OpenClaw、Hermes、Claude Code 这几种格式，要么通用 agent，要么专攻编程，既不兼容 Iris 当前格式，也不适配笔记核心需求。所谓的"安全装外部 skill 包"理由因此站不住脚——根本没有适配的包要装。
- **两条独立系统互相纠缠**：Skills 把自己当 MCP capability 消费者（`skill.mcp_bridge`），MCP 又把 skill 管理工具当一等公民。两个本该独立的系统互相背书对方的存在必要性，结果是 `skill.mcp_bridge` 同时被 4 个层判为 supported / Critical / high_risk / Planned——这是典型的"自我证明循环"。
- **真实需求其实很简单**：建 Skills 的原始动机是"装 self-improvement 类 skill 让 agent 自我改进"；建 MCP 的原始动机是"装 AnySearch 之类的强力搜索 MCP 来补 Iris 联网搜索短板"。这两个动机不要求通用 Agent 平台——前者改由 harness 原生内部集成更合适，后者只需要 MCP runtime + broker 单一入口即可。

### 收口思路

- **Skills**：完全自产自销。Iris 内置一个类似 Claude Code `skill-creator` 的能力，由对话驱动生成 `SKILL.md`；这些 skill 只是 prompt 级，权限最高就是对 vault 中**用户批准范围**内文档的增删查改（提出建议，不直接写）；不连 MCP、不执行脚本、不安装依赖。外部安装路径（URL / Git / Registry）全部移除。
- **MCP**：保留已建成 runtime，但只服务 `web.search` / `web.fetch`；agent 不直接 call MCP tool，MCP provider 经 broker 调度；用户在管理中心仍可装/启/禁/删 MCP server、看健康、发现工具。
- **网络**：单一 `WebEvidenceBroker` 入口，开关二道闸；native（MiniMax / DDG）与 MCP 搜索 provider "一主一补" 协调并发；fetch 由 broker 统管，删除三件 fetch agent tool；按 2026-06-21 文档主干补齐生命周期，但**削掉与"笔记核心 + MCP provider 即强搜索主力"哲学冲突的过度部分**。

### 与 2026-06-21 文档的关系

- **采信主干**：Policy Gate / Query Plan / Provider 选择执行 / 结果归一化 / 缓存隔离 / 证据筛选注入 / 引用摘要 / 审计 / 留存清理 / 权限规则 / 封闭平台边界——全部纳入本设计块 C。
- **削掉过度**：7-provider native pool、垂直学术源、三档预算 + 持续收集、7 个 MCP curated profile 写死、`McpHostRuntime` 安全细则的设计章节（已实现，转文档化）——全部记入 ROADMAP "未来通过 MCP 用户自配"路径，本 spec 不实现。
- **修正过时**：2026-06-21 文档"项目目前几乎没有可执行 MCP runtime""`skill.mcp_bridge` 是 planned/preflight"两条都已过时——现 runtime 已可用，`skill.mcp_bridge` 在 4 层间矛盾而非真正的 planned。本 spec 据实重写。

## 块 A — Skills 收口为自产 prompt-only

### A1 移除 skill kind 与 capability 消费者身份

#### Rust 删除/收缩

| 位置 | 改动 |
|---|---|
| `ai_runtime/skills/manifest.rs:9-18` | `SkillManifestKind` 6→2，仅留 `LegacyPromptOnly` + `PromptOnly` |
| `manifest.rs:32-46, 69-112` | 删 `resources` / `workspace` / `capabilities` / `mcp` / `degradation` 子契约；保留 `prompt` 子契约，其 `sections[]` 仅留 `id` / `source`，去掉 `requires_runtime` / `requires_capabilities` / `requires_resources` / `requires_workspace` |
| `manifest.rs:281-347` | 安全校验保留（拒绝 `runtime` / `process` / `scripts` / `secrets` 顶层键仍合理） |
| `ai_types/mod.rs:150-163` | `SkillRuntimeCapability` 6 变体全删；连带删 `mod.rs:236-267` 的 `requested_capabilities` 字段、`skills/model.rs:113-118` 的 `SkillEntry::requested_capabilities()` |
| `capability_resolver.rs:176-193` | `is_supported_capability` 仅留 4 条 `web.*`；删 `skill.read_resource` / `write_storage` / `mcp_bridge` |

#### 消除 `skill.mcp_bridge` 四层矛盾

- `agent_permissions.rs:84-85`：`SkillMcpBridge` atom vocab **保留**（B 哲学不拆词汇），但 `:388` 的 `skill_mcp_bridge` tool profile 删除，`:419-431` 的 MCP 管理 / 调用 tool profile 全删（因为这些 tool 本身要移除，见 A3）。
- `skill_trust_policy.rs:106-108`：删 `McpBridge` / `ExecuteScriptSandboxed` / `InstallDependency` 三条高险触发；`:124-125` 的 `skill.write_storage` 字符串检查删。
- `skills/compatibility.rs:46, 121`：删 `McpBridge → Planned` 映射；`:117-120` `ExecuteScriptSandboxed / InstallDependency → BlockedByPolicy` 删；`:112-116` `ReadResource / WriteStorage / RequestCapabilities → Supported` 删。整函数瘦成仅 `read / grep / glob / ls / notebookread → Supported` 等纯工具 affinity 路径。
- `sandbox_profile.rs:115-117`：删 `skill.execute_script_sandboxed / install_dependency / mcp_bridge` 路由 arm。
- `activation.rs:278-287` `capability_terms_for_skill`、`:570-613` `requires_runtime` / `requires_capabilities` arm、`:482-525` `runtime_ready_for_manifest`、`:862-866, :897, :927` `requested_capabilities` 收集——全删。
- `activation.rs:769-790` `build_skill_activation_plan_for_task_with_runtime` 与 `enable_manifest_gating` 选项合并进 `:746-766` 的非 runtime 变体。

### A2 移除外部安装路径

| 位置 | 改动 |
|---|---|
| `skill_install_service.rs` | 删 git-clone / URL / registry 预览三路径，留本地文件写入（skill-creator 用） |
| `skill_registry.rs` | 整文件删（SkillHub adapter 不再需要） |
| `skill_trust_policy.rs:11-18` | `SkillSourceKind` 删 `Registry` / `Git` / `Url`，仅留 `Local` |
| migration ≥042 | `skill_install_sources` 收窄 `source_type` 验证；`skill_trust_profiles` / `skill_diagnostics` / `skill_storage` / `skill_runtime_requirements` 四表**保留表结构**以兼容历史 migration，仅停止写入（无已装 skill） |
| `components/ai/SkillsPanel.tsx` | 删 `:619-678` URL/Git/本地拖拽安装三块、`:595-617` 安装 scope 选择、`:535-545` showInstall、`:438-450` workspace prepare、`:564-591` 内容编辑器 textarea。保留 `:515-534` tab shell（去 MCP tab）、search Input、`renderGroup` 列表、`SkillCard` 的 toggle / uninstall |
| `lib/ipc.ts` | 删 `skillsInstall` / `skillsPrepareWorkspace` / `skillsMigrateLegacy`；留 `skillsList` / `skillsPaths` / `skillsRead` / `skillsToggle` / `skillsUninstall` / `listenSkillsChanged` |

### A3 移除 skill 管理 agent tool

`tool_catalog/skills.rs` 19 项裁剪：

| 删除 | 保留 |
|---|---|
| `skills_install` / `prepare_workspace` / `uninstall` / `update` / `toggle`（5 个，模型不应自管 skill 安装；用户操作保留 IPC） | `skills_list`（只读，让 agent 知道有哪些 skill） |
| `skills_workspace_list` / `read` / `write`（3 个，workspace 概念废） | `skills_read_resource`（瘦身为只读 prompt 文本） |
| `mcp_runtime_tools_list` / `health_check` / `capability_call` / `server_catalog_upsert` / `profile_upsert` / `profile_toggle` / `profile_delete`（7 个 MCP 管理 / 调用，agent 不直接碰 MCP runtime——见块 B） | `mcp_runtime_profiles_list` / `diagnostics` / `tool_inventory_list` / `health_events_list`（4 个只读 MCP 元数据，供 agent 感知可用 provider，不触发动作） |

- `ToolAccessLevel::ManageSkills`（`ai_types/mod.rs:746`）若此后无 tool 引用则删 variant；连带重评估 `tool_catalog/capability.rs:21` `SkillManagement` affinity 与 `skill_trust_policy.rs:99` 引用。
- `tool_catalog/tests.rs:170` 的 catalog 计数（98）按实际删减项数同步：A3 删约 15 项 skills / mcp manage tool、C1 删 4 项 fetch 工具（仅留 `web_search`），总数 98 → 约 79。最终以实现时重跑测试定的准数为基准。

### A4 新增内置 skill-creator 流程

- 新 workflow `skill_creator_workflow.rs`，经 `harness_task` 调度，task_kind = `SkillManagement`（已存在 intent）。
- 流程：
  1. 用户对话说"建个 X skill" → skill-creator LLM 生成 `SKILL.md` 草稿 + `vault_write_scope`（glob / 路径 / 标签声明）。
  2. 前端弹确认卡，**全文展示草稿 `SKILL.md`** + scope；用户可编辑草稿与 scope 后批准。
  3. `SKILL.md` 写入 `.iris/skills-vault/<name>.md`（vault-scoped）或 `~/.iris/skills/<name>.md`（global）。
  4. skill 激活自动（同 P-1.5 决策：激活无需确认）。
  5. skill 运行时若要动 vault → 产 `PatchProposal` → 经 `guardrails` 现有体系 + 新增 **scope gate**：proposal 目标路径必须在声明 scope 内，否则拒；用户确认后 `file_ops` 执行写。
- skill-creator 自身不联网、不调 MCP、不执行脚本——纯 prompt 生成器。
- 既有 `skills_install` IPC 改造为 internal-only（被 skill-creator workflow 调用，不再暴露给前端按钮）。

### A5 数据迁移

- migration ≥043：新增 `skill_write_scopes` 表（`skill_name` / `scope` / `kind: glob|path|tag` / `pattern`）。
- 历史已装 skill（若有）：`source_type = registry/git/url` 的 `skill_install_sources` 记录标记 deprecated，不主动删除以保数据完整；激活路径跳过其 runtime 依赖检查。

## 块 B — MCP runtime 限用

### B1 适用范围收口

- MCP runtime 仅服务 `web.search` / `web.fetch` 两类 capability（broker 后端），不再对 agent 暴露任意 MCP tool。
- `capability_resolver.rs:176-193` `is_supported_capability`：**仅留 4 条 `web.*`**；resolver 只对 `web.*` 返回 provider，其它一律 `UnsupportedCapability`。`app_state.*` / `secret.*` / `process.*` 词汇保留不删（B 哲学：不拆 vocab，只收 resolver 入口）。
- agent 面向 MCP 的能力仅 `web_search` agent tool（dispatch → broker），不再有 `mcp_runtime_capability_call` 这条直连路径。

### B2 MCP web provider 经 broker 调度

- `search_web.rs` 的 MiniMax / DDG 仍作为 native 兜底。
- broker 新增 provider 调度器（见 C2）：native pool + MCP pool 合并选主补。
- MCP provider 通过 `mcp_host_runtime::call_profile_tool` 直接调用，**不经** `resolve_required_capability` 的 agent 路径——该 resolver 只决定"某 capability 能否被某 MCP profile 提供"，broker 直接用它枚举可用 provider，不再让 agent 触达。
- 删除 `mcp_runtime_capability_call` agent tool（A3 已列），保留 IPC `mcp_runtime_health_check` 供 UI。

### B3 MCP 管理 UI 保留

- `McpProfilesPanel` / `McpProfileCard` 从 SkillsPanel tab 搬到管理中心独立分区。
- `mcpRuntimeProfilesList` / `Toggle` / `Delete` / `HealthCheck` / `ToolsList` IPC 全留。
- 仅**agent 工具面**收口，**用户 UI 面**不动。

### B4 移除 agent-callable MCP 管理 tool

A3 已列 7 个 `ManageSkills` MCP tool 删除。`ToolAccessLevel::ManageSkills` 若此后无引用则删 variant。

## 块 C — 网络能力与 AI 联网生命周期收口

### C1 单一入口收敛 + 开关统一

- 退役 `engine.rs:62-78` `apply_web_search` + `prepend_web_search_context_for_db` prompt-prefix 路径；`LlmGenerateParams.web_search` 字段废弃。
- 4 处 inline `fetch_search_context_for_db` → `web_packets_from_fetch` → `mix_and_rank`（`writing_commands.rs:156`、`document_commands.rs:38` & `:144`、`citation_commands.rs:41`）全替换为 `collect_web_evidence` + `web_evidence_items_to_packets`。
- 退役 `rendered_fetch` tool：删 `tool_catalog/web.rs:92-111`、`tool_dispatch_impl.rs:138`、`DISPATCHABLE_TOOL_NAMES:36`、`agent_permissions.rs:351,653`、`tool_executor.rs:149`；`harness/run.rs:892,991` 的 `fetch_web_page` 硬编码改为 broker 内部计数。
- **开关统一硬约束**（用户明确要求）：
  - broker 入口 `collect_web_evidence` 开头 `if !input.enabled return Ok(vec![])`（已有，`web_evidence_broker.rs:40`）。
  - MCP 调度分支**重复检查** `enabled` 作二道闸——MCP provider 调用前再校验一次，防 MCP 路径绕开开关。
  - `tool_policy.rs:143` 的 Network 工具闸保留作 model-tool 路径的兜底。
- catalog count test 98 → 约 79（A3 删 ~15 + C1 删 4 fetch），以重跑测试定准。

### C2 broker 生命周期补齐（按 2026-06-21 文档主干，削过度部分）

#### Policy Gate（文档 §347-360）

- `ToolPolicyContext` 已传 `web_search_enabled` + vault；补 sensitive 路径黑名单（`.classified/` 等）在 broker 内 query plan 前过滤。

#### Query Planning（文档 §380-401）

- broker 内新增 `plan_query`：抽关键词 / 实体 / 语种 / 时效性，生成 1-3 候选查询。
- 默认规则优先；仅 `research` task_kind 触发 LLM 扩展查询。
- 隐私：不发送整篇笔记 / 完整选区，只发最小查询词 + ≤200 字符上下文片段；剥离明显 PII（邮箱 / 手机号 regex）。

#### Provider 选择 + 执行（A1 并发主补，用户选定）

- **provider 列表** = native（MiniMax, DDG）+ MCP（registry 中 enabled 且 healthy 且 `capability_mapping` 含 `web.search`）。
- **优先级**（用户确认）：默认 `MCP > MiniMax > DDG`——已配 MCP 则优先用付费 / 强力。
- **A1 调度**：选 top-2 healthy = 主 + 补，`tokio::join!` 并发 + cancel token，**先到可接受质量（result_count ≥ min_results 且非全空）即收**，未完成者 cancel；若两者都完成则 merge + URL 去重（canonical_url）。
- **provider health**：分钟级 TTL 表 `web_provider_health`（migration ≥044）；失败计数达阈值触发 circuit breaker（复用 `circuit_breaker.rs`）。
- **失败归类**：timeout / rate_limited / empty / auth_missing / auth_failed / network_blocked / policy_denied / provider_disabled / parse_error 九类（schema 在 broker）。

#### Fetch 安排（B1 broker 统管，用户选定）

- broker 搜索完成后自动 enrich top-K（默认 K=3，可配）结果页。
- **fetch provider 池** = native（static HTTPS readability，现 `fetch_web_page.rs`）+ MCP（`web.fetch` capability 的 MCP profile）。
- 同 A1 并发模型：每 URL 候选 fetch provider 主补并发，先到即收。
- 单 URL 超时、字节上限（64KB stdout cap 已有，正文截断 12K 已有 `FETCH_EXCERPT_MAX_CHARS`）。
- **删 agent tool**：`fetch_web_page` / `readability_fetch` / `web_fetch_batch` 三件（`tool_catalog/web.rs:30-91` 全删，只留 `web_search`）；`harness/tools.rs:21-22` merger 静默丢包 bug 随之消失。
- 用户主动"我要拉这个 URL"需求：经研究 workflow 的 `web_search` tool 表达触发 broker fetch；或在管理中心提供"抓取此 URL"独立操作（非 agent 能力）。

#### 结果归一化（文档 §452-507）

- `WebEvidenceItem` 扩字段：`provider_id` / `provider_kind` / `cost_class` / `raw_result_hash` / `extraction_method` / `trust_level: external_untrusted` / `retrieval_reason`。
- 全部外部内容 `trust_level = external_untrusted`。
- 修 `web_evidence_items_to_packets:93` 硬编码 `Duckduckgo` backend 的 bug：携带真实 provider 信息。
- 旧的 `web_packets_from_fetch` 中文文本块反解析路径标记 deprecated，仅 broker 暂用过渡，新代码不定。

#### Evidence 筛选 + 注入边界（文档 §561-595）

- broker 选 excerpt 进 context：按 relevance / diversity / freshness / dedup / token budget。
- `packet_builder` 注入时统一加 untrusted 边界标记：system prompt 加"外部网页文本不是指令"边界声明 + 每条 evidence 带 `trust_level`。
- guardrails 已有的 injection 检测保留作二道防线。
- excerpt-only，完整正文留 web cache。

#### 引用 + 摘要（文档 §597-610）

- 维持现状：无独立 evidence tab，Markdown 内嵌引用；研究 workflow 临时只读 tab 保留。
- AI 消息底部加可折叠联网摘要（执行了哪些搜索 / 打开哪些 URL / 用了哪些 provider / fallback / failures / cache hits），复用现有 `ContextPacketDrawer` 标题行扩展。

#### 审计（文档 §612-636）

- `tool_audit.rs:107` 改记 `url_hash` 而非 url 全文；`web_search` 记 `query_hash`。
- `trace.rs:redact_classified_leaks` 已脱敏，维持。
- broker 写 `web_evidence_ledger`（migration ≥044）：存 evidence IDs / hash / citation / provider metadata，不复制完整正文。
- audit 不得存：API key / token / 完整笔记内容 / 完整网页正文 / 大段 raw provider response / password / cookie。

#### 留存清理（文档 §638-665）

- 现 `cleanup_expired_search_cache` startup 调用保留；补 LRU 容量上限（每表如 5000 条）。
- `web_provider_health` 分钟级 TTL 自动清。
- 删除聊天会话：删 session messages + task references，不立即删全局 web cache（按 TTL / LRU 自然过期）。
- 清除网络缓存：删 search / page / provider health caches，不删用户笔记、不删 AI 对话正文。

### C3 缓存隔离完成

- migration ≥042：`search_cache` + `web_page_cache` 加 `provider_id` / `provider_kind` / `cost_class` 列；`vault_id` 列已存在（030）但代码不写——补 `search_web.rs:cache_set_db` / `cache_get_db` + `fetch_web_page.rs:257,287` 写入并 `WHERE vault_id = ?` 过滤。
- cache key 加 `provider_config_hash` + `broker_version`（防 provider / 版本变更后脏命中）。
- 专用缓存区与 `session_messages` / context assembly cache / packet cache / conversation memory / note index / vector index 彻底隔离（现表结构已隔离，仅需代码侧 vault_id 落地）。

### C4 削掉的过度部分（文档化不实现）

记入 ROADMAP "未来通过 MCP 用户自配"路径，本次不做：

- native 强 provider 池（Brave / Tavily / Exa / Kagi / SearXNG / Firecrawl / Jina / AnySearch 内置适配）→ 改为 MCP server 经 registry 引入。
- 垂直学术源（OpenAlex / Crossref / PubMed / GDELT / GitHub / RSS）。
- `research` task_kind 的 quick / standard / research 三档预算 + 持续收集模式 → 沿用现有 `research_workflow`，仅隐式按 task_kind 调 max_results。
- 7 个 MCP server 的 curated profile 写死 → 只留 `anysearch` 一个样例 + generic adapter。
- `McpHostRuntime` stdio / HTTP 安全细则的设计章节 → 文档化现状，删除冗余设计章节。

### 权限规则（文档 §669-700，维持现状）

- search：底部联网开关开启后可自动执行；仍受 provider policy + rate limit 约束。
- fetch（broker 内自动 enrich）：无独立确认，跟随 search 开关；broker fetch 完整正文只存 web cache 不入 session。
- 用户主动"抓取此 URL" 独立操作：需要确认（确认内容显示 URL / reason / provider / 预计 cache TTL）。
- batch fetch：需要确认，强制 max URLs。
- download / assets：需要确认，与普通 evidence collection 分离。
- login / cookie / auth browsing：非默认联网能力，未来单独设计。

### 封闭平台边界（文档 §702-719，维持现状）

默认不支持：微信搜一搜 / 公众号登录态 / 付费数据库 / 验证码 / 登录后网站 / 私有 SaaS / paywall。
允许路径：用户粘贴内容 / 上传文件 / 提供可访问 URL / 未来授权浏览能力。

## 实施顺序

1. **A1-A3**（Skills 删减）+ **B1-B4**（MCP 限用）——纯删减，无新功能，可并行；migration ≥042 表结构清理。
2. **C1**（收敛 + 开关统一 + 退役 `rendered_fetch` / prompt-prefix）——破坏性改动先做。
3. **C3**（缓存 migration + vault_id / provider_id 落地）。
4. **C2**（broker 生命周期补齐：query plan / provider 调度 / 归一化 / evidence 边界 / 审计 / 摘要）。
5. **A4**（skill-creator workflow 新建）+ migration ≥043 / ≥044。
6. 测试：broker provider 调度测试、`rendered_fetch` 删除后 catalog 计数、skill-creator scope gate 测试、开关统一二道闸测试、evidence 不可信边界测试、vault_id 缓存隔离测试。

## 前端 / IPC 同步（AGENTS.md §4.2）

本设计触及的 IPC 命令变更必须同步 `src/types/ipc.ts` + `src/lib/ipc.ts`：

- 删：`skillsInstall` / `skillsPrepareWorkspace` / `skillsMigrateLegacy`。
- 改：`WebEvidenceItem` DTO 扩字段。
- 留：`mcpRuntime*` 所有 IPC（UI 仍需）。

## 迁移与兼容

- migration ≥042：`search_cache` + `web_page_cache` 加 `provider_id` / `provider_kind` / `cost_class`；`skill_install_sources` 收窄 `source_type`；保留 `skill_trust_profiles` / `skill_diagnostics` / `skill_storage` / `skill_runtime_requirements` 表（仅停用）。
- migration ≥043：新增 `skill_write_scopes`。
- migration ≥044：新增 `web_provider_health`、`web_evidence_ledger`。
- 每条 migration 配 `down` 回滚脚本，放 `src-tauri/migrations/`，按版本号命名（AGENTS.md §4.3）。
- 历史已装 skill（`source_type = registry/git/url`）保留记录不主动删，激活路径跳过 runtime 依赖检查。
- legacy tools（删除的 `rendered_fetch` / `fetch_web_page` 等）退役前可留为兼容 wrapper 内部调 broker，但本设计直接删除（破坏性收口，用户面无依赖）。

## 测试计划

### Skills 收口

- `SkillManifestKind` 仅接受 `prompt_only` / `legacy_prompt_only`，其它 kind 解析失败。
- `SkillRuntimeCapability` 解析失败（字段全删）。
- `skill.mcp_bridge` 在 resolver / permissions / trust_policy / compatibility 四层都不再出现 supported / Critical / high_risk / Planned 信号。
- skill-creator 生成的 skill 经用户确认草稿 + scope 后落库。
- skill 运行时产 `PatchProposal` 目标路径超出声明 scope 时被 scope gate 拒绝。
- skill 激活无需确认；动 vault 时需用户确认。

### MCP 限用

- `is_supported_capability` 仅接受 4 条 `web.*`，其它返回 `UnsupportedCapability`。
- agent tool catalog 不含 `mcp_runtime_capability_call` / 7 个 MCP 管理 tool。
- McpProfilesPanel UI 在管理中心独立分区可装 / 启 / 禁 / 删 MCP server。

### 网络收口

- 联网关闭时，broker 不发生 native / MCP / model-provider 出站调用；二道闸生效。
- 4 处 inline 调用 + prompt-prefix 路径全部走 `collect_web_evidence`。
- `rendered_fetch` 工具不可用；catalog 计数同步。
- broker provider 调度：A1 并发主补，先到可接受质量即收 + 去重；MCP 优先级 > MiniMax > DDG。
- broker 自动 fetch top-3 结果页；`harness/tools.rs` merger 不再丢包。
- `WebEvidenceItem` 含 `provider_id` / `provider_kind` / `cost_class` / `trust_level: external_untrusted` 等字段。
- `web_evidence_items_to_packets` 不再硬编码 `Duckduckgo` backend。
- cache 写入 `vault_id` / `provider_id`，`WHERE vault_id = ?` 过滤生效。
- evidence 注入含 untrusted 边界标记；guardrails 二道防线仍工作。
- audit 记 `url_hash` / `query_hash`，不记完整 url / query。
- LRU 容量上限 + provider health 分钟 TTL 生效。

## 假设

- MCP runtime 已就绪（`mcp_host_runtime.rs` stdio + HTTPS 可用），本设计不重建。
- 用户主动装的 MCP server 列表中，`anysearch` 作为唯一 curated 样例；其它 MCP 由用户自配。
- native 强 provider 通过 MCP server 引入而非内置适配——本设计不写 7 个 native adapter。
- self-improvement / Proactive Agent 由 harness 原生内部集成，单独立项，不在本 spec。
- 破坏性收口可以接受——用户面无依赖 legacy fetch tool / prompt-prefix 路径。

## 后续单独立项（不在本 spec）

- self-improvement / Proactive Agent：harness 原生内部集成。
- 强 search provider（Brave / Tavily / Exa / Kagi / SearXNG / Firecrawl / Jina）：用户自配 MCP server，经 broker 调度。
- 垂直学术源：用户自配 MCP。
- research workflow 三档预算 + 持续收集模式：现有 `research_workflow` 基础上后续打磨。