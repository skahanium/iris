# Iris AI Agent 体系一体化路线图

> 状态：规划草案  
> 日期：2026-06-12  
> 关系：本文档暂不修改 `ROADMAP.md`。在确认后再决定是否同步到正式版本排期。

## 1. 总目标

Iris 的 AI 体系不再继续扩展为一组彼此割裂的“场景、模型、人设、skills、工具权限”入口，而收敛为一个面向本地 Markdown vault 的统一 Agent 系统。

最终形态：

- 用户侧只有一个主要 AI 助手入口，不再要求用户手动理解“场景切换 + 模型路由 + 人格切换 + skill 启用”的组合。
- Harness 在每次执行前生成 `AgentRunPlan`，明确本轮意图、模型槽位、人格层、工具、权限、skills、风险与确认点。
- 模型层按能力路由，而不是按固定厂商路由。文本、长上下文、推理、工具调用、视觉、私有本地模型都进入统一 capability registry。
- 人格层只影响表达、偏好、工作方式和提示词，不授予权限，不改变安全边界。
- Skills 是可诊断、可预检、可观测的 Agent 能力包；skill 不能绕过 ToolPolicy，也不能启用 Iris 未实现或未授权的能力。
- 权限底座服务于本地 Markdown Agent，而不是通用电脑控制 Agent。核心能力围绕 vault、Markdown、assets、网页资料、文档转换、受控 shell/git 展开。

## 2. 阶段拆分

### Phase 1: Agent Runtime Foundation

文档：`AI-AGENT-PHASE-1-RUNTIME-FOUNDATION.md`

目标是先统一底层执行模型：`AgentIntent`、`AgentRunPlan`、ToolPolicy、Permission Preflight、Trace/Audit、运行状态事件。后续所有入口、模型路由、skills、权限都挂到这个底座上。

### Phase 2: Unified Assistant

文档：`AI-AGENT-PHASE-2-UNIFIED-ASSISTANT.md`

目标是把用户侧的复杂场景入口合并为单一 AI 助手。场景保留为内部兼容层，用户通过自然语言、选择内容、当前笔记、命令动作触发意图识别。

### Phase 3: Model + Persona Routing

文档：`AI-AGENT-PHASE-3-MODEL-PERSONA-ROUTING.md`

目标是统一模型能力路由与人格注入：模型按 capability slot 选择，人格按 identity / style / task overlay 分层注入。模型、人格、意图三者协作，但都不能越过权限底座。

### Phase 4: Skills/Harness Closed Loop

文档：`AI-AGENT-PHASE-4-SKILLS-HARNESS-CLOSED-LOOP.md`

目标是让 skills 从“装上了但不知道有没有用”变成可安装、可激活、可预检、可诊断、可观测、可解释的闭环。Hermes 兼容也在这一阶段进入 capability mapping。

### Phase 5: Markdown Permission Base

文档：`AI-AGENT-PHASE-5-MARKDOWN-PERMISSION-BASE.md`

目标是建设最终版 Markdown Agent 权限底座。Iris 不追求通用电脑控制，而聚焦 vault、外部资料选择式授权、assets、文档处理、网页采集、受控 shell/git、skill sandbox。

### Cross-cutting: UI/UX Plan

文档：`AI-AGENT-UI-UX-PLAN.md`

这不是第六阶段，而是横切体验规范。它统一规划 AI 侧栏、inline AI、RunPlan 抽屉、确认中心、模型/人设/skills/权限设置页、诊断与审计视图，保证五个阶段实现时不把界面再次拆散。

## 3. 总体架构

```text
User
  -> Unified Assistant Entry
  -> Intent Classifier
  -> AgentRunPlan
       -> Model Capability Router
       -> Persona Resolver
       -> Skill Activation Planner
       -> ToolPolicy + Permission Preflight
       -> Context Planner / Retrieval Broker
  -> Harness Executor
       -> LLM Adapter
       -> Tool Dispatcher
       -> Permission Gate
       -> Confirmation Checkpoint
       -> Trace / Audit
  -> Response / Patch / Artifact / Diagnostic UI
```

核心边界：

- `AgentRunPlan` 是执行前唯一计划对象。
- `ToolPolicy` 是工具暴露和执行的硬边界。
- `Permission Preflight` 是每轮权限授权、阻断和确认的硬边界。
- `Trace/Audit` 只记录元数据和摘要，不记录 API Key、笔记正文、图片 base64、剪贴板正文、敏感 shell 输出。
- `PromptProfile` / Persona 不参与权限决策。
- Skill manifest 只能声明需要什么能力，不能自己授予能力。

## 4. 跨阶段公共类型

建议逐步收敛出这些公共概念：

- `AgentIntent`：用户意图，如 chat、ask_notes、rewrite_selection、research、organize、citation_check、vision_chat、skill_management。
- `AgentRunPlan`：本轮执行计划，包含 intent、context scope、model slots、persona layers、skills、tools、permissions、confirmation points。
- `CapabilityRoute`：模型能力槽，如 fast、writer、reasoner、long_context、vision、agent_tools、local_private。
- `PersonaLayer`：identity、style、task_overlay、skill_overlay、safety_overlay。
- `SkillActivationPlan`：本轮激活 skill、匹配原因、注入内容、requested capabilities、blocked capabilities。
- `AgentPermission`：vault、fs、doc、web、skill、process、git、clipboard、browser 等权限原子。
- `PermissionPreflight`：允许、需确认、阻断、缺失实现、缺失用户授权。
- `AgentDiagnostic`：对用户可见的运行诊断，不泄露敏感内容。

## 5. 实施顺序与依赖

顺序采用“先底座再体验”：

1. Phase 1 建立运行计划、工具策略、权限预检、审计事件。
2. Phase 2 把用户入口合并到单一助手，但复用 Phase 1 的运行计划。
3. Phase 3 重构模型路由与人格注入，使它们成为 RunPlan 的组成部分。
4. Phase 4 让 skills 通过 RunPlan 和 PermissionPreflight 进入可执行闭环。
5. Phase 5 扩展权限原子和工具执行层，最终支撑 Markdown Agent 的高级本地能力。

这个顺序牺牲一部分早期可见体验，但返工最少。

## 6. 非目标

- 不建设 Obsidian 式通用插件市场或任意 UI 扩展运行时。
- 不建设通用电脑控制 Agent。
- 默认不开放全盘文件写入、全局键鼠控制、读取系统环境变量、明文读取凭据。
- 不允许 skill 动态注册任意 Rust 工具或绕过 ToolCatalog。
- 不让模型、人设或 prompt 文本决定权限。

## 7. 验收标准

- 用户无需手动切换场景即可完成查阅、写作、研究、整理、引用检查、skill 管理、带图问答。
- 每次 Agent 执行前后都能看到清晰的计划、进度、工具使用和阻断原因。
- 模型路由可解释：为什么选 fast/writer/reasoner/vision/agent_tools。
- 人格行为一致，且不会重复注入或与 skill overlay 混乱叠加。
- Skill 是否激活、为什么激活、能否执行、缺什么权限都能直接看到。
- Markdown vault 写入可预览、可确认、可回滚。
- 安全日志不泄露密钥、笔记正文、附件 base64、剪贴板正文。
