# Phase 1: Agent Runtime Foundation

> 状态：规划草案  
> 目标：先统一 Agent 执行底座，避免入口、模型、人格、skills、权限各自为政。

## 1. 目标

建设 Iris AI Agent 的统一运行时基础：

- 每次执行先形成 `AgentRunPlan`。
- 所有工具暴露、权限检查、确认点、trace、audit 都由 Harness 统一编排。
- 保留现有工作流能力，但把它们逐步迁移到统一 Agent runtime 上。
- 为后续单一助手入口、模型路由、人格分层、skills 闭环、Markdown 权限底座提供稳定接口。

## 2. 核心类型

### AgentIntent

用于替代用户显式选择的高层场景：

- `chat`
- `ask_notes`
- `rewrite_selection`
- `write`
- `research`
- `organize`
- `citation_check`
- `chapter`
- `document_check`
- `vision_chat`
- `skill_management`

旧 `AiScene` 保留为内部兼容层，短期内由 `AgentIntent` 映射到旧 workflow。

### AgentRunPlan

每轮执行前生成：

- request id / session id
- detected intent
- selected context scope
- selected capability routes
- selected persona layers
- selected skills and activation reasons
- exposed tools
- required permissions
- confirmation checkpoints
- token budget and round limits
- expected output type: answer、patch、batch suggestions、research note、diagnostic

### PermissionPreflight

用于在执行前说明：

- auto allowed
- requires confirmation
- blocked by missing grant
- blocked by unsupported capability
- blocked by policy
- blocked by model capability

## 3. ToolPolicy 收敛

现有 ToolPolicy 继续作为硬边界，但需要从单一 `ToolAccessLevel` 扩展到：

- required permissions
- risk level
- confirmation policy
- data exposure class
- supported intent affinity
- side effect class: read、write、network、process、external

策略原则：

- skill 不能启用未实现工具。
- skill 不能绕过 confirmation。
- model/persona 不能扩大工具权限。
- 同一轮 RunPlan 中只计算一次可用工具集合，后续 O(1) 查询。
- 写入、外部访问、联网下载、命令执行都必须进入 audit。

## 4. Harness 执行流

统一执行流：

```text
assistant_execute
  -> build AgentRunPlan
  -> permission preflight
  -> context assembly
  -> model request
  -> tool call parse
  -> tool policy check
  -> confirmation checkpoint if needed
  -> tool dispatch
  -> model continuation
  -> final response / patch / diagnostic
  -> trace + audit finalize
```

关键要求：

- parse retry 有上限。
- 工具有超时。
- tool result 要结构化。
- 用户拒绝确认后，Harness 要能继续生成解释或替代方案，而不是直接中断崩溃。
- checkpoint 需要保留 provider 必须回传的 tool call 上下文和 reasoning metadata。

## 5. Trace 与 Audit

Trace 面向调试，Audit 面向安全解释。

记录：

- request id
- intent
- model slot / provider / model id
- selected skills
- tool names
- permission decisions
- confirmation decisions
- duration
- token usage
- error code

禁止记录：

- API Key / token
- 用户笔记正文
- 图片 base64
- 剪贴板正文
- 外部文件正文
- shell 原始敏感输出

## 6. UI 可见性

Phase 1 不要求重做全部 UI，但需要给后续 UI 暴露：

- `run_plan`
- `permission_preflight`
- `tool_status`
- `skill_activation_plan`
- `blocked_capabilities`
- `audit_summary`

前端可以先在开发/诊断面板显示，不必一开始进入最终视觉形态。

## 7. 测试计划

- RunPlan 生成测试：不同 intent 能得到稳定 capability routes、tools、permissions。
- ToolPolicy 测试：未实现工具、场景不匹配、权限不足、skill allowlist 缺失都能被阻断。
- Confirmation 测试：写入工具、外部访问、联网下载、命令执行进入确认。
- Harness 恢复测试：确认通过、拒绝、修改参数后都能继续。
- 安全测试：trace/audit/session 不写入敏感正文和密钥。
- 超时测试：网络、文件、命令类工具超时后返回结构化错误。

## 8. 验收标准

- 任意 AI 执行都能产出可解释的 `AgentRunPlan`。
- 工具可用性只由 ToolPolicy 和 PermissionPreflight 决定。
- 现有 workflow 行为不回退。
- 安全日志通过敏感数据检查。
- 后续阶段可以不绕过 Phase 1 直接接入。
