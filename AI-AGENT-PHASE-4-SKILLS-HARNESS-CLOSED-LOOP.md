# Phase 4: Skills/Harness Closed Loop

> 状态：规划草案  
> 目标：让 skills 从提示词包变成可预检、可激活、可诊断、可执行闭环的一等 Agent 能力。

## 1. 目标

解决当前用户感知中的核心问题：

- skill 安装后不知道是否真的启用。
- skill 被启用后不知道是否影响了本轮回答。
- skill 声明的工具 Iris 不支持时，只能失败或弱提示。
- Hermes 中可运行的 skill 到 Iris 中因缺少权限和运行时能力而不可用。
- Harness 对 skill 的执行路径缺少可视化和诊断闭环。

## 2. Skill Manifest 扩展

在兼容 `SKILL.md` 的基础上，提取或补充：

- name
- description
- trigger hints
- allowed tools
- requested capabilities
- required resources
- optional resources
- sandbox requirements
- external dependencies
- license
- compatibility source: iris、claude、hermes、unknown

`allowed-tools` 继续支持，但不再是唯一能力描述。

## 3. Activation Planner

每轮 RunPlan 中生成 `SkillActivationPlan`：

- matched skills
- match reason
- score
- injected sections
- requested tools
- requested capabilities
- unsupported capabilities
- blocked capabilities
- resources available
- resources too large or truncated

匹配策略：

- explicit user mention 优先。
- UI 手动启用覆盖自动匹配。
- BM25 粗筛 + embedding 重排。
- active skill 数量限制，避免 prompt 污染。
- 不逐轮无限重匹配，除非用户任务明显切换。

## 4. Hermes 兼容映射

新增 Hermes compatibility mapping：

```text
Hermes tool/capability
  -> Iris AgentPermission
  -> Iris ToolCatalog entry
  -> support status
  -> risk level
  -> fallback guidance
```

状态分为：

- supported
- supported_with_confirmation
- planned
- unsupported_by_product_scope
- blocked_by_policy
- missing_user_grant

重点不是盲目兼容 Hermes 的全部电脑控制能力，而是明确告诉用户：这个 skill 缺哪个能力，Iris 为什么不能执行，是否有 Markdown 工作台替代能力。

## 5. Skill Runtime 能力

分层支持：

- `skill.read_resource`：读取 skill resources/references/assets。
- `skill.write_storage`：每个 skill 独立存储目录。
- `skill.request_capabilities`：声明本轮所需权限。
- `skill.execute_script_sandboxed`：高级能力，默认关闭，需要 sandbox、cwd 限制、env 脱敏、超时、确认。
- `skill.install_dependency`：高风险，单独确认，记录依赖和许可证。
- `skill.mcp_bridge`：后续扩展点，不在本阶段强制落地。

## 6. 诊断 UI

Skills 页面展示：

- installed / enabled
- last matched
- last used
- activation score
- requested capabilities
- unsupported capabilities
- confirmation-required capabilities
- resource read status
- script execution status
- compatibility warnings

AI 本轮回答展示：

- 本轮激活了哪些 skills。
- 每个 skill 为什么被激活。
- 哪些 skill 被阻断。
- 阻断原因是未授权、未实现、超出产品范围，还是安全策略。

## 7. Harness 闭环

闭环流程：

```text
install/scan skill
  -> validate license and manifest
  -> build activation index
  -> preview capabilities
  -> user enable/disable
  -> RunPlan activates skill
  -> PermissionPreflight checks requested capabilities
  -> resources/scripts/tools execute through ToolPolicy
  -> diagnostics and audit visible
```

## 8. 测试计划

- 安装测试：local/git/url/registry 安装后生成 capability preview。
- 激活测试：显式提及、自动匹配、手动覆盖均生效。
- 映射测试：Hermes 常见 tool names 映射到 Iris permissions 或明确 unsupported。
- 阻断测试：未知工具、未授权权限、script execution 关闭时都有明确诊断。
- 资源测试：超长 resource 截断，不写入敏感日志。
- 安全测试：skill 不能绕过 ToolPolicy 和 confirmation。
- UI 测试：Skills 页面和本轮 RunPlan 都能显示 skill 状态。

## 9. 验收标准

- 用户能直观看到 skill 是否安装、启用、匹配、参与本轮执行。
- Hermes skill 不再“沉默失败”，而是给出能力映射和阻断原因。
- 支持 Markdown 工作台范围内的 skill 扩展能力。
- 不支持的通用电脑控制能力被明确标注为产品非目标或待扩展。
