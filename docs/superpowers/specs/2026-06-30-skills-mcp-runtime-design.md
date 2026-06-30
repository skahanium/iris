# Iris Skills 与 MCP Runtime 终局体系设计

日期：2026-06-30
状态：设计基线

## 1. 背景与问题判断

Iris 现在暴露出来的问题，不是单个 SkillHub 安装按钮、某个 Skill 文案、某个确认弹窗的局部 bug。真正的问题是：当前 Skills 体系把“给模型看的行为说明”和“真实可执行的运行能力”混成了一种状态。

这会导致三类错觉：

1. 简单 `SKILL.md` 被误认为必须有 runtime、workspace 或 MCP，结果本来应该可用的 prompt skill 被诊断成不完整。
2. 复杂 Skill 只要安装成功就被 UI 和 Agent 说成“可用”，但它依赖的搜索、daemon、脚本、长期记忆、外部服务其实没有被 Iris 接管。
3. 工具确认完成后，如果 assistant resume 或 provider thinking replay 失败，UI 把“后续生成失败”误报成“工具执行失败”，用户自然会以为安装被改坏。

Iris 要修的不是“让这一次 anysearch/heartflow 看起来能过去”，而是补上一个明确的能力基座：

- Skill 是受控 AI 能力包。
- MCP 是可选执行后端。
- Iris ToolCatalog 和 ToolPolicy 是模型可调用能力的唯一边界。
- 安装、启用、激活、运行、workspace、确认、审计必须是分层状态，不能继续压成一个“可用”。

## 2. 核心结论

**Skills 体系必须同时支持无 MCP 的简单 Skill 和依赖 MCP/Provider 的复杂 Skill。MCP 不能成为所有 Skill 的地基，只能成为执行能力的一种 provider。**

因此，以下两条路径都必须是一等公民：

1. **简单 Skill 路径**：只有 `SKILL.md`，没有 `iris.skill.toml`，没有 MCP，没有 workspace。它仍然可以安装、启用、匹配、读取、注入，并显示为行为层可用。
2. **复杂 Skill 路径**：通过 typed manifest 声明资源、workspace、capability、MCP/profile 依赖。它可以安装和启用，但 runtime 未就绪时只能部分激活，不能假装执行能力已部署。

这也是对“如果安装的 skill 很简单完全不依赖 MCP，这个体系是否有效”的回答：有效，而且这是本体系的底线。不依赖 MCP 的 Skill 不应该被 MCP 状态拖累。

## 3. 设计目标

1. 让 `SKILL.md` only 的 prompt-only Skill 成为一等公民。
2. 让复杂 Skill 用 typed manifest 表达真实依赖，而不是靠模型阅读正文猜测。
3. 拆开 `installed`、`enabled`、`activation_ready`、`runtime_ready`、`workspace_prepared`、`availability`。
4. 建立 Iris-controlled MCP registry，禁止 Skill 私自携带 shell/daemon 作为可执行事实。
5. 让模型只调用稳定的 Iris capability，不直接消费任意 MCP tool name。
6. 让 MCP profile、provider health、tool inventory、capability mapping、confirmation、audit 全部有持久化和测试。
7. 让安装成功但 resume 失败成为明确的 partial success，而不是误报工具失败。
8. 为未来 MCP、原生 provider、插件化 provider 留出空间，但不牺牲简单 Skill 的低门槛体验。

## 4. 非目标

- 不构建 Obsidian 式前端插件 API。
- 不允许 Skill 扩展 TipTap schema、菜单、页面路由或任意 UI。
- 不自动执行 Skill 包内的 `start.sh`、`install.sh`、`npm install`、`npx`、Hermes daemon 或 OpenClaw runtime。
- 不把 MCP tool 原样暴露给模型。
- 不把 API key、token、cookie、raw env 写入 manifest、SQLite、日志、prompt 或确认详情。
- 不把旧 Hermes/OpenClaw 的外部运行目录当作 Iris 沙箱内能力直接继承。
- 不为了让某个 Skill “显示可用”而伪造 runtime、workspace 或 `reasoning_content`。

## 5. 能力分层

### 5.1 Behavior Layer

由 `SKILL.md` 提供，负责方法论、风格、步骤、约束、领域知识。

特征：

- 给模型读。
- 不执行外部副作用。
- 可被 activation 注入上下文。
- 可以独立可用。

典型例子：代码审查风格、写作方法、调试流程、heartflow 的行为范式。

### 5.2 Resource Layer

由 `references/`、`resources/`、`templates/` 等只读材料提供。

特征：

- required resource 缺失时会影响 activation。
- optional resource 缺失只产生 warning。
- 读取必须走 Skills resource API，不能让模型猜路径。

### 5.3 Workspace Layer

由 `.iris/skills-workspaces/<skill>/` 提供，保存派生文档、自检产物、模板状态或 skill 私有工作文件。

特征：

- `workspace_declared` 和 `workspace_prepared` 分离。
- `files=[]` 只表示当前没有派生文件，不表示 workspace 未准备。
- 写入必须通过受控工具和确认。
- workspace 内容不是用户笔记权威源，不得混入 vault 普通笔记语义。

### 5.4 Runtime Layer

由 Iris 原生 provider 或 MCP Host Runtime 提供，负责搜索、抓取、计算、进程调用、第三方工具调用。

特征：

- 必须由 Iris 注册、校验、授权、审计。
- Skill 只能声明需要 capability 或 profile，不能内嵌任意 shell 启动命令。
- runtime 缺失不阻止安装，但会导致相关 section blocked/degraded。

### 5.5 Agent Tool Layer

由 Iris ToolCatalog 暴露给模型。

特征：

- 模型只能看到 Iris 工具或稳定 capability。
- MCP inventory 不能直接变成模型 tool list。
- 所有写操作、进程启动、网络 runtime、registry 修改都必须走 ToolPolicy 和 confirmation。

## 6. Skill 类型

### 6.1 `legacy_prompt_only`

没有 `iris.skill.toml`，只有 `SKILL.md`。

契约：

```text
kind = legacy_prompt_only
validation = legacy
runtime_kind = not_applicable
runtime_ready = true
workspace_declared = false
workspace_prepared = false
availability = available | unavailable
mcp_dependencies = []
```

可用条件：文件可读、元数据可解析、已安装、已启用。

### 6.2 `prompt_only`

有 typed manifest，但声明不依赖 runtime/workspace。

契约：

```text
kind = prompt_only
validation = valid
runtime_kind = not_applicable
runtime_ready = true
workspace_declared = false
availability = available
```

### 6.3 `resource`

需要只读资源。

契约：required resource 缺失时 `availability = partial | unavailable`，并列出 `missing_resources`；optional resource 缺失只进入 `warnings`。

### 6.4 `workspace`

需要派生工作区。

契约：

```text
workspace_declared = true
workspace_prepared = true | false
generated_files_count = number
workspace_missing_items = string[]
```

`generated_files_count = 0` 不等于失败。

### 6.5 `mcp_dependent`

需要 MCP 或 provider capability。

契约：

```text
runtime_kind = mcp | provider
runtime_ready = true | false
runtime_status = ready | degraded | unavailable | blocked | unknown
availability = available | partial | unavailable
blocked_capabilities = []
mcp_dependencies = []
```

安装成功不等于 runtime ready。

### 6.6 `hybrid`

同时包含行为层、resource、workspace、runtime。

契约：每个 prompt section 必须按 gate 激活。可用 section 注入，blocked section 用降级说明替代。

heartflow 属于这类：行为范式可以激活，Node daemon、dream loop、长期记忆闭环如果没有被 Iris runtime 接管，就必须显示为 runtime unavailable。

## 7. 文件契约

推荐目录：

```text
skill-name/
  SKILL.md
  iris.skill.toml
  references/
  resources/
  templates/
```

`SKILL.md` 是人类和模型可读入口。frontmatter 只放轻量信息：

```yaml
---
name: anysearch
version: 1.0.0
description: 网络检索能力包
iris_manifest: iris.skill.toml
---
```

没有 `iris.skill.toml` 时，Iris 不猜 runtime、不猜 MCP、不猜 workspace，按 `legacy_prompt_only` 处理。

Prompt-only manifest 示例：

```toml
schema_version = "1"
name = "legal-review"
version = "1.0.0"
kind = "prompt_only"

[prompt]
default_sections = ["behavior"]

[[prompt.sections]]
id = "behavior"
source = "SKILL.md"
requires_runtime = false

[workspace]
declared = false

[capabilities]
requires = []

[degradation]
when_runtime_missing = "not_applicable"
```

MCP-dependent manifest 示例：

```toml
schema_version = "1"
name = "anysearch"
version = "1.0.0"
kind = "mcp_dependent"

[prompt]
default_sections = ["behavior", "web-search-usage"]

[[prompt.sections]]
id = "behavior"
source = "SKILL.md"
requires_runtime = false

[[prompt.sections]]
id = "web-search-usage"
source = "SKILL.md#web-search-usage"
requires_runtime = true
requires_capabilities = ["web.search", "web.fetch"]

[workspace]
declared = false

[capabilities]
requires = ["web.search", "web.fetch"]

[[mcp.dependencies]]
profile_id = "anysearch"
required_capabilities = ["web.search", "web.fetch"]
required = true

[degradation]
when_runtime_missing = "partial"
message = "AnySearch MCP profile 未启用时，只注入检索方法说明，不执行联网检索。"
```

禁止字段：

```text
command
shell
script
install
start
api_key
token
secret
password
raw_env
headers.Authorization
```

这些字段如果出现在 runtime、mcp、permissions、workspace、capabilities 等安全敏感区，应直接 validation error；普通 metadata 未知字段只 warning。

## 8. 状态模型

Skill UI、IPC、Agent 诊断必须表达以下状态，禁止用单个“可用”吞掉细节：

```text
installed: bool
scope: global | vault
enabled: bool
validation: valid | legacy | invalid
kind: prompt_only | legacy_prompt_only | resource | workspace | mcp_dependent | hybrid
activation_ready: bool
runtime_kind: not_applicable | mcp | provider | unavailable
runtime_ready: bool
runtime_status: ready | degraded | unavailable | blocked | unknown
availability: available | partial | unavailable
workspace_declared: bool
workspace_prepared: bool
workspace_missing_items: string[]
generated_files_count: number
degraded_reasons: string[]
blocked_sections: string[]
activated_sections: string[]
blocked_capabilities: BlockedCapabilitySummary[]
mcp_dependencies: McpDependencySummary[]
```

解释规则：

- `installed && enabled` 不等于可用。
- `validation = valid` 不等于 runtime ready。
- prompt-only 的 runtime 是 `not_applicable`，不是 missing。
- MCP-dependent 在 MCP 缺失时可以安装/启用，但 `runtime_ready = false`。
- workspace 未声明时，不显示“已准备”；workspace 已声明但未创建时，才显示 missing。
- `files=[]` 是派生文件计数，不是 readiness 判断。

## 9. Activation 与注入

Skill activation 必须按 section gate 执行：

1. 解析用户任务意图、已启用 Skill、匹配度。
2. 读取 manifest 或 legacy prompt-only 摘要。
3. 计算 resource/workspace/runtime/capability readiness。
4. 注入满足 gate 的 sections。
5. blocked sections 只注入简短降级说明，不注入不可执行操作说明。
6. run plan 或诊断结果记录 activated sections、blocked sections、degraded reasons。

Gate 输入：

```text
requires_runtime
requires_capabilities
requires_resources
requires_workspace
```

Gate 输出：

```text
activated_sections
blocked_sections
degraded_reasons
blocked_capabilities
```

## 10. MCP Registry 与 Host Runtime

MCP profile 是独立 runtime provider：

```text
profile_id
display_name
source: curated | user | local_dev
scope: global | vault
transport: stdio | https | sse
enabled
trust_level
credential_binding_id?
declared_capabilities[]
allowed_tools[]
denied_tools[]
health_status
last_error
created_at
updated_at
```

Host Runtime 负责：

- validate / enable / disable / delete profile
- initialize
- tools/list
- tools/call
- health check
- timeout / cancellation
- stdout/stderr/output caps
- sanitized audit
- credential binding injection through OS credential manager

安全要求：

- stdio 必须是结构化 `command + args`，禁止 shell string。
- 禁止自动调用 `npm`、`npx`、`pnpm`、`yarn`、`bun` 等包管理器。
- env 默认清空，只允许 credential binding 注入。
- HTTPS/SSE 默认必须 HTTPS；localhost 仅 dev mode；私网、metadata endpoint 和危险 redirect 默认拒绝。
- Host Runtime 失败必须归一化为稳定错误码，不把原始 stderr、网页正文、token 或长输出返回给 Agent。

## 11. Capability Mapping

模型面对的是 Iris capability，不是任意 MCP tool。

稳定能力词汇至少包括：

```text
web.search
web.fetch
web.to_markdown
web.download_to_assets
skill.read_resource
skill.write_storage
skill.mcp_bridge
app_state.read
app_state.write
secret.exists
secret.use_named
process.run_readonly
process.long_running
```

映射规则：

- Iris 原生工具由 ToolCatalog 映射到 capability。
- MCP inventory 只通过 curated/user-approved mapping 映射到 capability。
- MCP annotations 只能作为提示，不能自动授予 capability。
- ToolPolicy 是模型工具暴露的唯一硬边界。
- MCP `tools/call` 必须经过 capability resolver、permission preflight、confirmation、audit，再进入 Host Runtime。

## 12. 安装、确认与错误语义

安装流程：

1. 拉取或复制 Skill 包。
2. 解析 `SKILL.md` frontmatter。
3. 若存在 manifest，解析并严格校验。
4. 计算 resource/workspace/capability/MCP preflight。
5. 写入 Skill 文件与 install source metadata。
6. 刷新 activation index。
7. 写入 runtime status snapshot。
8. 返回结构化 outcome。

确认流程拆分两层结果：

```text
tool_execution_outcome:
  status: succeeded | failed | rejected
  side_effect_committed: bool
  tool_name
  result_summary

assistant_resume_outcome:
  status: resumed | skipped | failed
  failure_class?
  user_message?
```

如果 `skills_install` 已提交成功，但 assistant resume 因 provider reasoning replay 或网络失败而失败，UI 必须显示：

```text
安装已完成，但继续生成回复失败。
```

不能显示：

```text
工具确认失败
Skill 安装失败
```

## 13. UI 信息架构

管理中心 AI 分区拆为：

- Skills
- MCP / Providers

Skills 卡片显示：

- 名称、版本、scope、source、enabled
- validation、kind、availability
- activation/runtime/workspace 状态
- activated/degraded/blocked sections
- MCP dependencies 与 health
- 最近一次使用、匹配、阻塞原因

MCP / Providers 显示：

- profile 列表
- transport、scope、trust、enabled
- health、last error、tool inventory
- credential binding 状态
- 启停、诊断、删除入口

文案规则：

- 不用“当前可用”概括全部状态。
- prompt-only 不显示 runtime 缺失。
- MCP missing 显示“缺少或未启用 MCP profile”，不显示“Skill 坏了”。
- workspace 未声明不显示“已准备”。

## 14. 与 Hermes/OpenClaw 的差异

Hermes 或 OpenClaw 可以“安装后直接跑”，通常是因为它们接受了更大的 runtime 假设：外部目录、Node 运行时、脚本、daemon、进程生命周期、环境变量和网络能力由宿主环境默认承担。

Iris 不能照搬这个模型，原因是：

- Iris 是本地笔记与 AI Agent 结合的桌面应用，安全边界更接近“用户知识库管家”。
- Iris 有明确的 AGPL、本地数据、凭据管理、确认、审计要求。
- Iris 不能让 Skill 文本把任意脚本声明成可执行能力。
- Iris 未来要支持 MCP/provider，但必须通过统一 runtime registry，而不是把每个 Skill 变成小型应用安装器。

因此 Iris 的正确方向不是“更像 Hermes 的自由执行”，而是“把 Hermes/OpenClaw 这类外部能力显式接入为受控 provider”。

## 15. 测试矩阵

必须覆盖：

- `SKILL.md` only 简单 Skill 可安装、启用、激活、注入，且无 MCP warning。
- typed manifest prompt-only 走同一状态语义。
- MCP-dependent Skill 在 MCP 缺失时安装成功但 runtime unavailable。
- section-level injection gate：可用 section 注入，blocked section 降级。
- workspace declared/prepared/generated files count 的语义。
- MCP registry profile upsert/toggle/delete/list persistence。
- stdio/HTTPS/SSE Host Runtime 的 tools/list、tools/call、timeout、output cap、failure normalization。
- capability resolver 不把 MCP annotations 自动授予模型。
- ToolPolicy 不直接暴露 MCP tool。
- 工具确认 partial success 不误报 side effect failure。
- UI 不把 prompt-only、workspace、runtime、MCP 状态混淆。

## 16. 验收标准

- 简单 `SKILL.md` Skill 在没有 manifest、没有 MCP、没有 workspace 的情况下完整可用。
- anysearch 这类 MCP-dependent Skill 可以安装和启用，但只有 MCP profile ready 后才注入执行层说明。
- heartflow 这类 hybrid/meta Skill 可以保留行为层激活，同时明确执行层 runtime 缺失。
- Agent 可诊断 MCP registry 和 health，但不能绕过确认修改 registry 或启动进程。
- MCP profile 管理、live tools/list、health check、tools/call 均有权限、确认、审计和测试。
- 安装成功但 resume 失败时，UI 真实显示 partial success。
- 文档、IPC、Rust/TS 类型、测试共同定义契约，不再靠临时文案解释行为。
