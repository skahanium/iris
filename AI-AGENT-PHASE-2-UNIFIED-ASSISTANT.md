# Phase 2: Unified Assistant

> 状态：规划草案  
> 目标：把用户侧 AI 入口收敛为单一助手，隐藏复杂场景切换。

## 1. 目标

Iris 当前存在场景、workflow、模型路由、人格、skills 等多个概念。Phase 2 的目标是让用户不再先选择“场景”，而是通过一个统一 AI 助手自然表达任务。

用户只需要：

- 输入问题或任务。
- 选择文本或当前笔记。
- 使用 slash / context menu / AI panel 的具体动作。
- 在必要时确认写入、联网、导入、权限。

Harness 负责把这些输入解析为 `AgentIntent` 和 `AgentRunPlan`。

## 2. 用户入口

统一入口包括：

- AI 侧栏主输入框。
- 选中文本后的 inline AI。
- 右键 AI 菜单。
- slash command。
- 文件/目录/知识图谱中的 AI 动作。
- Skills 管理自然语言请求。

这些入口都调用同一条 runtime pipeline，只是带上不同 input hints。

## 3. 场景处理

旧 `AiScene` 不直接暴露给用户。

保留方式：

- `AiScene` 作为内部兼容层。
- `AgentIntent` 映射到旧 workflow。
- 旧场景相关 session、history、tool affinity 做迁移兼容。
- UI 中不再出现“知识查阅 / 文稿学习 / 文稿创作 / 学术研究”的主切换器。

短期映射示例：

- `ask_notes` -> KnowledgeLookup
- `rewrite_selection` / `write` -> DraftingAssist
- `research` -> ResearchSynthesis
- `organize` -> KnowledgeLookup or DraftingAssist, depending on scope
- `citation_check` -> ResearchSynthesis
- `vision_chat` -> new vision route, fallback to chat if no image

## 4. Intent 识别

Intent 来源按优先级合并：

1. 明确 UI action，如 rewrite、summarize、citation check。
2. 当前上下文，如 selection、note path、folder scope、attached images。
3. 用户自然语言。
4. active skills 的 trigger hints。
5. 默认 chat。

识别结果必须可解释：

- detected intent
- confidence
- reason
- possible alternatives
- fallback behavior

低置信度不应弹复杂选择器；可通过一句轻量追问或使用默认 chat + suggest actions。

## 5. 对话体验

AI panel 需要展示：

- 当前助手身份。
- 当前任务意图标签。
- 上下文范围 chips。
- 附件 chips。
- 本轮 run plan 摘要。
- 工具/权限确认。
- 引用与证据抽屉。
- patch preview。

不要把运行时内部概念暴露成复杂配置。高级信息放入诊断视图。

## 6. 与模型/人格/skills 的关系

Unified Assistant 只负责入口和意图，不直接决定模型、人设或权限：

- 模型由 Phase 3 的 capability routing 决定。
- 人格由 Phase 3 的 persona resolver 决定。
- skills 由 Phase 4 的 activation planner 决定。
- 权限由 Phase 1 + Phase 5 的 policy/preflight 决定。

## 7. 迁移策略

- 旧场景 session 继续可读。
- 历史记录按 intent 显示新标签，内部仍可保留 scene 字段。
- 旧 UI 场景 selector 先隐藏，不立即删除底层类型。
- IPC 保持兼容，新增字段走可选参数。

## 8. 测试计划

- 不同入口都能生成同一类 `assistant_execute` 请求。
- 选中文本 rewrite 自动识别 `rewrite_selection`。
- 当前笔记问题自动识别 `ask_notes`。
- 带图请求自动识别 `vision_chat`。
- skill 安装请求自动识别 `skill_management`。
- 低置信度 intent 有合理 fallback。
- 旧 session 可继续打开和续聊。

## 9. 验收标准

- 用户不需要理解场景切换即可完成主要 AI 工作流。
- UI 不再同时暴露“场景切换 + 模型路由 + 人格切换”的复杂组合。
- 每轮任务都有可解释的 intent 和 run plan。
- 旧工作流能力不丢失。
