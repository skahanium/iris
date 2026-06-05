# AI Harness 重构计划

> **状态**：规划修订版  
> **创建日期**：2026-06-05  
> **修订日期**：2026-06-05  
> **范围**：Agent Harness、Skills Runtime、PromptBuilder、工具治理、安全审计、前端 AI 设置体验  
> **总目标**：在不降低现有能力的前提下，把 Iris 的 AI 体系打造成贴合本地 Markdown 笔记工作流的一流 Agent 系统。  
> **方法论**：分阶段 TDD。每个阶段先写失败测试，再做实现，再跑完整质量检查。

---

## 0. 设计立场

这份计划维持原始目标：Iris 不只是“接入 LLM 的笔记应用”，而应成为一个本地优先、证据可追溯、工具受控、人格可配置、可通过 Skills 扩展的一流 Agent 笔记系统。

修订重点不是收缩目标，而是把目标落到当前仓库的真实结构上：

- 保留 Tauri 2.x + Rust + React 19 + TipTap + TailwindCSS + shadcn/ui + SQLite/sqlite-vec 技术栈。
- 用户 `.md` 文件仍是笔记知识的最高权威来源。
- 所有写入 `.md` 的 AI 行为必须经过用户明确确认。
- Skills 可以扩展 Agent 能力，但不能绕过工具硬权限、场景约束、自动化等级和用户确认。
- PromptProfile 应能真正覆盖默认人格，而不是仅追加在硬编码 persona 之后。
- 审计日志必须避免记录 API Key、Token、笔记全文、加密密码等敏感内容。

---

## 1. 当前事实校正

| 编号 | 原计划判断 | 修订判断 | 影响 |
| --- | --- | --- | --- |
| F-01 | `read_note` 可任意读取系统文件 | 不准确。`read_note` 和 `get_outline` 已使用 `is_user_note_path` + `resolve_vault_path`；真实缺口在路径校验未统一，`get_backlinks`、`get_block_links` 等 DB 路径参数未走同一入口 | Phase 1 改为补齐统一校验，不重复造一套不兼容路径逻辑 |
| F-02 | `install_from_git` 是 shell 命令注入风险 | 不准确。当前使用 Rust `Command::args`，不是 shell 拼接；真实风险是缺少 `--` 参数分隔、`subpath` 未 canonicalize、复制源边界未限制在临时 clone 目录内 | Phase 1 改为参数与路径边界加固 |
| F-03 | 迁移编号从 `014` 开始 | 错误。仓库已有 `014_web_page_cache` 到 `017_rename_cascade` | 新迁移从 `018` 起，并同步 `storage/migrate.rs` |
| F-04 | 新增 `src-tauri/src/ipc.rs` | 错误。当前 Tauri 命令在 `src-tauri/src/commands/ai_commands.rs` 等 commands 模块 | 所有 IPC 后端变更写入 commands 模块 |
| F-05 | 新增 `src/components/settings/SkillsPanel.tsx` | 不贴合现状。仓库已有 `src/components/ai/SkillsPanel.tsx` 和 `src/lib/ipc.ts` 的 skills 封装 | 先复用并升级现有面板，再决定是否搬到 settings |
| F-06 | 内置工具清单“19 个” | 不一致。计划表实际列出 21 个；代码中还区分 dispatchable tool 与 harness-only tool | 新增 ToolCatalog 明确工具注册、暴露、执行、确认四层状态 |
| F-07 | 用 Skills 完全替代 `scene_allowlist` | 风险过高。`scene_allowlist` 当前承担硬场景边界 | 改为 ToolPolicy 硬约束优先，Skills 只能在硬约束交集内请求启用工具 |
| F-08 | sqlite-vec 直接作为 skill embedding 唯一路径 | 不完整。`sqlite-vec` 当前是 optional feature，迁移也有 best-effort 先例 | Skill 匹配需要 sqlite-vec 路径和 Rust fallback 路径 |
| F-09 | `allowed-tools` 自动授权 | 表述过强。Agent Skills 规范中 `allowed-tools` 是 experimental，且不同客户端支持不同 | Iris 解释为“skill 请求的工具集合”，最终权限由 ToolPolicy 裁决 |
| F-10 | PromptProfile 可覆盖人格 | 当前不成立。`PromptProfile` 只是环境片段，`ModelGateway::unified_persona` 仍硬编码「砚」 | Phase 3 建 PersonaResolver/PromptBuilder |

---

## 2. 目标架构

### 2.1 分层模型

```text
User Request
  -> SceneRouter
  -> PersonaResolver
  -> SkillActivator
  -> ToolPolicy / ToolCatalog
  -> PromptBuilder
  -> Harness Loop
  -> Tool Dispatch / Confirmation / Audit
  -> Evidence Ledger
  -> Final Answer / Patch Proposal
```

### 2.2 核心模块

| 模块 | 职责 | 现有基础 | 目标 |
| --- | --- | --- | --- |
| ToolCatalog | 定义所有内置工具、访问级别、是否已实现、是否 harness-only、是否需确认 | `ToolRegistry`、`ToolSpec`、`DISPATCHABLE_TOOL_NAMES` | 统一工具事实来源，消除工具清单不一致 |
| ToolPolicy | 计算当前请求可暴露/可自动执行/需确认的工具集合 | `scene_allowlist`、`requires_confirmation`、`AutonomyLevel` | 硬约束优先，Skills 只能做交集内扩展 |
| SkillRuntime | 扫描、校验、安装、启停、匹配、渐进式加载 skills | `ai_runtime/skills.rs`、现有 SkillsPanel | 兼容 Agent Skills 规范，同时保留 Iris 元数据 |
| SkillActivator | 根据用户请求、scene、history、description 匹配激活 skills | `trigger` 简单匹配 | 支持 description 关键词 + 向量匹配 + 显式用户启用 |
| PersonaResolver | 解析默认人格、用户 PromptProfile、场景侧重 | `PromptProfile`、`ModelGateway::unified_persona` | PromptProfile 可完全覆盖默认身份 |
| PromptBuilder | 统一 harness 与 workflow 的 prompt 构建 | `harness/context.rs`、`model_gateway.rs` | Base Persona、Scene Focus、Skills、Evidence、User Rules 分层 |
| HarnessControl | 管理轮次、确认、反思、token budget、checkpoint | `harness/run.rs` | 修复死代码，补 usage fallback，子 agent 受控反思 |
| ToolAudit | 持久化工具调用审计，带敏感信息脱敏 | `ai_traces` | 可查询、可调试、不可泄露隐私 |

---

## 3. 阶段拆解

### Phase 0：基线校正与测试护栏

> **目标**：先把计划和仓库事实对齐，建立后续重构的失败测试入口。  
> **预估**：1-2 天。  
> **外部行为**：不改变。

**任务**

- 修订计划中所有错误路径、迁移编号、工具数量、前端组件位置。
- 为以下事实补测试或快照：
  - `read_note` 拒绝 `../`、绝对路径、`.iris/` 元数据路径。
  - `get_backlinks`、`get_block_links` 对 `.iris/`、绝对路径、`../` 输入应拒绝。
  - `ToolRegistry` 中所有可自动暴露工具必须有 dispatch handler 或 harness-only handler。
  - `skills_list` 返回类型与 `src/lib/ipc.ts` 保持一致。

**验收**

- `cargo test storage::paths`
- `cargo test ai_runtime`
- `npm run typecheck`

---

### Phase 1：Harness 与安全缺陷修复

> **目标**：修复当前 AI loop 的硬缺陷，不改变 Skills 大架构。  
> **预估**：3-5 天。  
> **原则**：小步、可回滚、每项缺陷独立测试。

#### 1.1 修复 `pending_confirmation` 死变量

**现状**

`src-tauri/src/ai_harness/harness/run.rs` 中 `pending_confirmation` 初始化为 `false` 后未改变；实际确认流程已经通过 `first_pending_confirmation_call` 和 `return_pending_confirmation` 存在另一条路径。

**修订方案**

- 删除主循环尾部对静态 `pending_confirmation` 的无效判断。
- 以 `pending_tool_call` / `return_pending_confirmation` 作为唯一暂停出口。
- 确保 `HarnessRunResult.pending_confirmation` 只在确认请求已写入 checkpoint 且前端事件已发出时为 `true`。

**测试**

- `test_pending_tool_call_returns_pending_result`
- `test_read_only_tools_do_not_pause`
- `test_multiple_confirm_tools_pauses_first_and_keeps_checkpoint`

#### 1.2 Token usage fallback

**现状**

当 provider 不返回 `usage.total_tokens` 时，`token_budget` 判断会失真。

**修订方案**

- 新增 `src-tauri/src/ai_harness/harness/token_estimator.rs`。
- 不新增依赖，先用保守字符估算：
  - CJK 字符按 `0.8` token。
  - ASCII 单词按粗略 `chars / 4`。
  - tool result JSON 按字符数上限估算。
- 仅当 provider usage 全部为 0 或缺失时启用 fallback。
- 在 trace 中记录 `usage_source: provider | estimated`，但不记录完整 prompt 或笔记全文。

**测试**

- provider 返回 usage 时不覆盖。
- provider 返回 0 时使用估算。
- budget 超限时 harness 停止继续发起模型请求。

#### 1.3 路径校验统一

**现状**

`storage::paths::resolve_vault_path` 已存在，应复用而不是另起一套 `path_validator`。

**修订方案**

- 在 `src-tauri/src/storage/paths.rs` 增加面向 AI 工具的轻量 helper：
  - `validate_user_note_relative_path(relative: &str) -> AppResult<()>`
  - `resolve_user_note_path(vault: &Path, relative: &str) -> AppResult<PathBuf>`
- `read_note`、`get_outline` 改用 helper，保持行为不变。
- `get_backlinks`、`get_block_links` 增加同样校验。
- 不允许 AI 工具读取 skills 目录；skills 文件读写继续走 `skills::validate_skill_path` 和专用 IPC。

**测试**

- `read_note("../../etc/passwd")` 被拒绝。
- `get_backlinks(".iris/versions/x.md")` 被拒绝。
- `get_block_links("../note.md")` 被拒绝。
- vault 内合法 `.md` 路径正常。

#### 1.4 `install_from_git` 边界加固

**修订方案**

- `git clone` 使用 `git clone --depth 1 -- <repo_url> <tmp>`。
- `subpath` 必须是相对路径，不允许 root、prefix、`..`。
- `tmp.join(subpath).canonicalize()` 后必须 `starts_with(tmp_canonical)`。
- copy 前若目标目录已存在，使用安全覆盖策略：先复制到临时 sibling，再原子替换，避免半写入。
- `repo_url` 继续使用 `security::ipc_policy::validate_skill_git_url`。

**测试**

- URL 以 `-` 开头被拒绝。
- `subpath = "../x"` 被拒绝。
- `subpath` symlink 跳出 clone 目录被拒绝。
- 合法单 skill 和多 skill repo 安装成功。

#### 1.5 子 agent 受控反思

**修订方案**

- 子 agent 允许一次 reflection，但必须满足：
  - `depth > 0` 时最多一次 bonus round。
  - 子 agent token budget 使用父任务分配的独立预算。
  - 反思不得触发新的 subagent 树。

**测试**

- depth 0 保持现有反思能力。
- depth 1 可反思一次。
- depth 2 不继续派生子 agent。

**Phase 1 验收命令**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

---

### Phase 2：ToolCatalog 与 ToolPolicy

> **目标**：让工具治理成为一流 Agent 的硬安全边界。  
> **预估**：4-6 天。  
> **外部行为**：默认工具可见性不退化。

#### 2.1 建立 ToolCatalog

**新增文件**

- `src-tauri/src/ai_runtime/tool_catalog.rs`

**职责**

- 定义每个工具的：
  - `name`
  - `description`
  - `input_schema`
  - `access_level`
  - `requires_confirmation`
  - `implemented`: dispatchable / harness-only / planned
  - `default_enabled_without_skill`
  - `scene_affinity`

**修订点**

- `ToolRegistry::new()` 从 ToolCatalog 构建。
- `DISPATCHABLE_TOOL_NAMES` 与 catalog 建立一致性测试。
- 附录工具清单以 catalog 为唯一来源生成或手动同步。

#### 2.2 建立 ToolPolicy

**新增文件**

- `src-tauri/src/ai_runtime/tool_policy.rs`

**权限计算顺序**

```text
implemented/harness-only hard gate
  ∩ scene affinity
  ∩ autonomy level
  ∩ web_search_enabled
  ∩ skill allowed-tools request
  ∩ user settings
```

**关键原则**

- Skills 不能启用未实现工具。
- Skills 不能绕过 `requires_confirmation`。
- Skills 不能在 L1 自动执行写入类工具。
- 无 skill 激活时保留核心默认只读工具：
  - `search_hybrid`
  - `search_semantic`
  - `search_keyword`
  - `read_note`
  - `list_vault`
  - `get_outline`
  - `get_backlinks`
  - `conclude_reasoning`

#### 2.3 `scene_allowlist` 的处理

不在本阶段直接从 `ToolSpec` 删除 `scene_allowlist`。先引入 `scene_affinity` / `scene_policy`，让新旧路径并行，测试一致后再在 Phase 4 删除旧字段。

**Phase 2 验收**

- 工具清单数量与代码一致。
- 所有 exposed tools 都有执行路径或 harness-only 路径。
- Drafting scene 仍可看到写入工具，但写入工具仍需确认。
- KnowledgeLookup scene 不暴露不相关写入工具，除非用户显式 skill + policy 允许，且仍需确认。

---

### Phase 3：Agent Skills Runtime

> **目标**：兼容 Agent Skills 规范，同时保留 Iris 的本地优先、安全可控、可配置体验。  
> **预估**：8-12 天。

#### 3.1 Skill 数据模型

**修订 `src-tauri/src/ai_runtime/skills.rs`**

```rust
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub metadata: SkillMetadata,
    pub allowed_tools: Vec<String>,
    pub content: String,
    pub scope: SkillScope,
    pub source_url: Option<String>,
    pub enabled: bool,
    pub file_path: String,
    pub legacy_trigger: Option<String>,
}
```

`trigger` 不再作为新格式字段，但读取旧文件时保留为 `legacy_trigger`，用于兼容和迁移提示。

#### 3.2 Frontmatter 解析

**要求**

- 新格式必须有 YAML frontmatter。
- `name` 必须与父目录名一致。
- `description` 必须非空且不超过 1024 字符。
- `compatibility` 不超过 500 字符。
- `metadata` 作为 `HashMap<String, serde_json::Value>` 存储，但 UI 只编辑已知字段。
- `allowed-tools` 是空格分隔字符串，解析后必须全部存在于 ToolCatalog。

**兼容策略**

- 旧格式带 `trigger` 可以继续读取。
- UI 显示“旧格式”提示。
- 自动迁移必须用户确认，不在后台静默改写用户 skill 文件。

#### 3.3 渐进式加载

**加载层级**

- Metadata：启动时只加载 name、description、scope、enabled、allowed-tools、metadata。
- Instructions：激活 skill 后加载 `SKILL.md` body，建议主文件小于 500 行。
- Resources：仅在 skill 内容明确引用 `references/`、`scripts/`、`assets/` 时按需读取。

**路径安全**

- Skill 资源读取只允许在 skill root 内。
- 不允许 skill 通过相对路径读取 vault 笔记，除非走正式 AI 工具且经过 ToolPolicy。

#### 3.4 Skill 匹配

**匹配策略**

1. 显式用户选择的 skill 优先。
2. description 关键词/BM25 风格匹配作为默认路径。
3. sqlite-vec 可用时增加向量 rerank。
4. sqlite-vec 不可用时使用 Rust 侧 cosine fallback 或关键词排序。
5. `legacy_trigger` 仅作为旧 skill 的弱提示，不作为新规范字段。

**新增存储**

当前仓库迁移已到 `017`，新增迁移从 `018` 起：

- `018_skill_install_sources.sql`
- `019_skill_activation_index.sql`

Skill embedding 不应直接假定 sqlite-vec 永远可用。建议：

- 普通表 `skill_activation_index` 保存 `skill_name`、`scope`、`description`、`keywords`、`embedding_json`、`updated_at`。
- 如果 `vector_index_ready()` 为 true，再创建 best-effort `vec_skill_descriptions`。

#### 3.5 依赖管理

`metadata.depends` 可作为 Iris 扩展字段，但不是 Agent Skills 官方核心字段。安装依赖时：

- 只解析同一安装源中明确存在的 skill。
- 不自动联网安装未知依赖。
- 缺依赖时在 UI 提示用户，而不是后台拉取。

#### 3.6 Skills UI

**复用现有文件**

- `src/components/ai/SkillsPanel.tsx`
- `src/lib/ipc.ts`

**升级能力**

- 显示规范状态：valid / legacy / invalid。
- 显示 allowed-tools 是否全部被 ToolCatalog 识别。
- 显示依赖缺失。
- 支持用户确认迁移旧格式。
- 可从设置面板打开同一个 SkillsPanel，而不是重复做一个 settings 版本。

**Phase 3 验收**

- `skills-ref validate` 能通过新建标准 skill。
- 旧 trigger skill 可读、可用、可提示迁移。
- skill 激活不会突破 ToolPolicy。
- 无 skill 时默认核心工具仍可用。
- `npm run lint`
- `npm run format:check`
- `npm run typecheck`
- `cargo test`

---

### Phase 4：PromptBuilder 与人格完全覆盖

> **目标**：统一 harness 与 workflow 的 prompt 构建，让用户可真正定义 Agent 身份。  
> **预估**：5-7 天。

#### 4.1 PersonaResolver

**新增文件**

- `src-tauri/src/ai_runtime/persona_resolver.rs`

**规则**

- 当 `PromptProfile.persona` 为空时，使用默认「砚」人格。
- 当 `PromptProfile.persona` 非空时，用户 persona 成为主身份；默认「砚」仅作为产品能力说明，不再强行声明名字。
- `writing_style`、`language`、`custom_rules` 分层加入 prompt。
- 场景侧重只描述当前任务能力，不覆盖 persona。

#### 4.2 PromptBuilder

**新增文件**

- `src-tauri/src/ai_runtime/prompt_builder.rs`

**分层**

```text
System Layer 1: Persona
System Layer 2: Product/Data Principles
System Layer 3: Scene Focus
System Layer 4: Tool Policy Summary
System Layer 5: Active Skills
System Layer 6: Evidence Packets
System Layer 7: User Rules
```

**替换路径**

- `src-tauri/src/ai_harness/harness/context.rs`
- `src-tauri/src/ai_runtime/model_gateway.rs`
- `src-tauri/src/ai_runtime/environment.rs`
- `src-tauri/src/ai_workflows/*.rs`

#### 4.3 回归测试

- 先写 prompt snapshot 测试覆盖现有 scene。
- 重构后确保：
  - 默认用户仍看到「砚」人格。
  - 自定义 persona 不再被硬编码「砚」覆盖。
  - web_search 关闭时 prompt 明确禁止 `web_search` / `fetch_web_page`。
  - active skills 只注入已激活 skills。

---

### Phase 5：确认流、PatchProposal 与审计

> **目标**：让 AI 写入变成可预览、可追溯、可恢复的受控编辑流程。  
> **预估**：5-8 天。

#### 5.1 ToolAudit

**新增迁移**

- `020_tool_audit.sql`
- `020_tool_audit.down.sql`

如果 Phase 3 已使用 `020`，则按实际最新编号顺延；提交前必须检查 `src-tauri/migrations/` 和 `storage/migrate.rs`。

**表设计**

```sql
CREATE TABLE IF NOT EXISTS tool_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    harness_round INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_summary TEXT,
    result_summary TEXT,
    success INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    scene TEXT,
    subagent_depth INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (request_id) REFERENCES ai_traces(request_id)
);
```

**敏感信息规则**

- 禁止记录 API Key、Token、密码。
- 禁止记录笔记全文。
- `read_note` 只记录 path、max_chars、truncated，不记录 content。
- `replace_selection` / `insert_text_at_cursor` 只记录长度、hash、风险级别，不记录完整文本。

#### 5.2 PatchProposal 集成

当前 `PatchProposal` 已在 `ai_types/mod.rs` 定义，但未成为工具确认主路径。

**目标**

- 写入 `.md` 的工具逐步迁移到 PatchProposal。
- 前端确认弹窗展示 diff 摘要。
- 用户可以 approve / reject / modify。
- 应用 patch 前验证 `base_content_hash`，避免覆盖用户并发修改。

**前端复用**

- 优先复用 `src/components/ai/PatchPreview.tsx` 和 `ToolConfirmDialog.tsx`。
- 只有现有组件无法承担通用 diff 时，再新增 `src/components/ui/diff-preview.tsx`。

---

### Phase 6：一流 Agent 体验收束

> **目标**：把底层能力收束成用户可感知的高级 Agent 体验。  
> **预估**：8-12 天。

**能力目标**

- 多步任务：Harness 可明确展示计划、工具调用、证据链、待确认编辑。
- 可追溯回答：引用本地笔记、网页缓存、规则来源。
- 可配置身份：用户可在设置中选择或自定义 Agent 身份。
- 可控扩展：Skills 可安装、验证、启停、迁移、查看工具请求。
- 安全默认：无 skill、无联网、低自动化等级时仍然安全可用。

**前端入口**

- 设置面板新增 AI Skills 入口，打开现有 SkillsPanel。
- AI 面板展示 active skills 和当前 tool policy 摘要。
- 工具确认弹窗展示 diff、风险提示、执行原因。
- Trace/审计视图可按 request_id 查看工具调用摘要。

---

## 4. 数据模型变更

当前基线已有迁移 `001` 到 `017`。新增迁移必须从最新编号后开始，提交前重新检查。

建议顺序：

```text
018_skill_install_sources.sql
018_skill_install_sources.down.sql
019_skill_activation_index.sql
019_skill_activation_index.down.sql
020_tool_audit.sql
020_tool_audit.down.sql
```

如果在开发期间已有新迁移合入，编号顺延。

每个 migration 必须：

- 加入 `src-tauri/src/storage/migrate.rs` 的 `include_str!`。
- 加入 `migrate_up`。
- 加入 `migrate_down`。
- 有 roundtrip 或 existence 测试。

---

## 5. 测试与质量门禁

### 5.1 Rust

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
```

### 5.2 TypeScript / React

```bash
npm run lint
npm run format:check
npm run typecheck
npm run test
```

### 5.3 E2E / 手工验证

```bash
npm run test:e2e
```

至少覆盖：

- AI chat 正常完成。
- 写入工具触发确认。
- 拒绝工具后 harness 能继续或正常结束。
- approve 后 patch/hash 校验通过。
- 自定义 persona 不再显示硬编码「砚」身份。
- 安装新格式 skill 后可激活。
- 旧格式 trigger skill 可提示迁移。

---

## 6. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| Skills 扩权导致安全退化 | 高 | ToolPolicy 硬约束优先，allowed-tools 只取交集 |
| PromptBuilder 改动面大 | 高 | 先写 snapshot，再替换路径 |
| sqlite-vec optional 导致 skill 匹配不可用 | 中 | 默认关键词匹配，sqlite-vec 仅作为增强 |
| 旧 skill 自动迁移损坏用户文件 | 中 | 不后台静默改写，迁移需用户确认并生成备份 |
| tool_audit 泄露敏感信息 | 高 | 只记录 summary/hash/长度，不记录正文和密钥 |
| 前端重复造面板 | 中 | 复用现有 SkillsPanel，只新增入口和状态展示 |
| 子 agent 反思增加成本 | 低 | 独立 budget，depth 限制，禁止递归派生 |

---

## 7. 验收标准

### 系统能力

- [ ] Harness 不再存在无效 `pending_confirmation` 死分支。
- [ ] provider usage 缺失时 token budget 仍能生效。
- [ ] 子 agent 有独立 token budget，并最多执行一次受控反思。
- [ ] 所有 AI 文件读取入口统一经过 vault/user-note 校验。
- [ ] `install_from_git` 有 repo URL、参数分隔、subpath、copy 边界校验。
- [ ] ToolCatalog 与 dispatch/harness-only 实现保持一致。
- [ ] Skills 兼容 Agent Skills 新格式，并兼容旧 trigger 格式。
- [ ] Skills 的 `allowed-tools` 不能绕过 ToolPolicy。
- [ ] PromptProfile 可完全覆盖默认「砚」人格。
- [ ] PromptBuilder 统一 harness 与 workflow 的 prompt 路径。
- [ ] 写入 `.md` 的 AI 操作进入 PatchProposal/diff/确认流程。
- [ ] tool_audit 可查询工具调用摘要，且不记录敏感内容。

### 用户体验

- [ ] 无 skill 时默认只读能力不退化。
- [ ] 用户可查看 active skills。
- [ ] 用户可查看 skill 请求了哪些工具，以及哪些被 policy 拒绝。
- [ ] 用户可确认旧 skill 迁移。
- [ ] 用户可配置 Agent 身份、语言、风格和长期规则。
- [ ] 工具确认弹窗展示清晰的 diff 和风险提示。

### 质量门禁

- [ ] `cargo fmt --all -- --check` 通过。
- [ ] `cargo clippy --all-targets -- -D warnings` 通过。
- [ ] `cargo test` 通过。
- [ ] `npm run lint` 通过。
- [ ] `npm run format:check` 通过。
- [ ] `npm run typecheck` 通过。
- [ ] `npm run test` 通过。
- [ ] 新增依赖均完成 AGPL-3.0 兼容性检查；优先不新增依赖。

---

## 8. 受影响文件清单

### Phase 1

```text
Modify:
  src-tauri/src/ai_harness/harness/run.rs
  src-tauri/src/ai_harness/harness/types.rs
  src-tauri/src/ai_harness/harness/util.rs
  src-tauri/src/ai_runtime/tool_dispatch.rs
  src-tauri/src/ai_runtime/skills.rs
  src-tauri/src/storage/paths.rs

Create:
  src-tauri/src/ai_harness/harness/token_estimator.rs
```

### Phase 2

```text
Create:
  src-tauri/src/ai_runtime/tool_catalog.rs
  src-tauri/src/ai_runtime/tool_policy.rs

Modify:
  src-tauri/src/ai_runtime/tool_executor.rs
  src-tauri/src/ai_runtime/tool_dispatch.rs
  src-tauri/src/ai_runtime/environment.rs
  src-tauri/src/ai_types/mod.rs
```

### Phase 3

```text
Modify:
  src-tauri/src/ai_runtime/skills.rs
  src-tauri/src/commands/ai_commands.rs
  src-tauri/src/storage/migrate.rs
  src/lib/ipc.ts
  src/types/ipc.ts
  src/components/ai/SkillsPanel.tsx
  src/components/settings/SettingsPanel.tsx

Create:
  src-tauri/migrations/018_skill_install_sources.sql
  src-tauri/migrations/018_skill_install_sources.down.sql
  src-tauri/migrations/019_skill_activation_index.sql
  src-tauri/migrations/019_skill_activation_index.down.sql
```

### Phase 4

```text
Create:
  src-tauri/src/ai_runtime/persona_resolver.rs
  src-tauri/src/ai_runtime/prompt_builder.rs

Modify:
  src-tauri/src/ai_runtime/model_gateway.rs
  src-tauri/src/ai_runtime/environment.rs
  src-tauri/src/ai_harness/harness/context.rs
  src-tauri/src/ai_workflows/*.rs
  src-tauri/src/ai_runtime/prompt_profile.rs
```

### Phase 5

```text
Modify:
  src-tauri/src/ai_harness/harness_confirm.rs
  src-tauri/src/ai_harness/harness/run.rs
  src-tauri/src/commands/ai_commands.rs
  src-tauri/src/ai_types/mod.rs
  src/components/ai/ToolConfirmDialog.tsx
  src/components/ai/PatchPreview.tsx

Create:
  src-tauri/migrations/020_tool_audit.sql
  src-tauri/migrations/020_tool_audit.down.sql
```

---

## 9. 外部规范参考

- Agent Skills Specification: https://agentskills.io/specification
- Iris 版本排期唯一来源：[ROADMAP.md](./ROADMAP.md)
- Iris UI token 与组件规范：[docs/design-system.md](./docs/design-system.md)
- Iris 文档索引：[docs/README.md](./docs/README.md)

---

_文档结束_
