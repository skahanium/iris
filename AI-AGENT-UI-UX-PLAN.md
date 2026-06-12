# Iris AI Agent UI/UX 统一方案

> 状态：规划草案  
> 日期：2026-06-12  
> 关系：本文档是 `AI-AGENT-ROADMAP.md` 的横切体验方案，覆盖 Phase 1 到 Phase 5 的界面规划。暂不修改正式 `ROADMAP.md`。

## 1. 体验目标

AI 体系的界面目标不是把所有底层概念摊给用户，而是把复杂运行时压缩成一个可信、可解释、可控制的 Markdown 工作台助手。

最终体验：

- 用户只面对一个主要 AI 助手入口。
- 任务意图、上下文、模型路由、人格、skills、权限都可解释，但默认不打扰。
- 需要用户决策时，界面只呈现必要信息：将读什么、写什么、运行什么、风险是什么、如何撤销。
- 所有 AI 写入都能预览、确认、回滚。
- Skills 和权限不再是黑箱，用户能看到是否激活、为什么激活、哪里被阻断。
- 高级诊断信息可展开查看，但不挤占日常写作和阅读体验。

## 2. 信息架构

AI 相关界面分为六个区域：

1. **Unified Assistant Panel**：日常对话、写作、研究、整理的主入口。
2. **Inline AI Surface**：选中文本、slash command、右键菜单触发的轻量入口。
3. **Run Plan Drawer**：本轮执行计划、模型、工具、skills、权限和进度的解释层。
4. **Confirmation Center**：写入、权限、联网、命令、skill 安装的统一确认体验。
5. **Agent Settings**：模型路由、人设、权限、skills、诊断配置。
6. **Diagnostics / Audit View**：面向高级用户的运行记录、阻断原因和安全摘要。

这些区域共享同一套运行时数据：

- `AgentRunPlan`
- `AgentIntent`
- `PermissionPreflight`
- `SkillActivationPlan`
- `ToolAudit`
- `ModelRouteDecision`
- `PersonaResolution`

## 3. Unified Assistant Panel

### 布局

AI 侧栏采用稳定的三段结构：

```text
Header
  assistant identity
  task intent
  compact status

Conversation
  messages
  evidence references
  patches
  tool progress
  diagnostic summaries

Composer
  input
  context chips
  attachment chips
  action buttons
```

Header 不再放场景切换器。它只显示：

- 助手称呼和状态。
- 当前自动识别的任务意图。
- 本轮是否使用模型、工具、skills、联网、视觉。
- Run Plan 展开按钮。

### Composer

Composer 必须支持：

- 多行输入。
- 当前笔记上下文 chip。
- 选中文本 chip。
- vault scope chip。
- corpus/folder scope chip。
- 临时图片附件 chip。
- 当前 note images 显式加入视觉上下文的 chip。
- web/search toggle。
- submit / stop / retry。

Composer 不显示模型下拉、人设下拉或场景切换。这些属于设置或诊断，不属于日常发送动作。

### 消息区

消息类型：

- 普通回答。
- 带引用回答。
- 写入建议。
- Patch proposal。
- Research result。
- Skill diagnostic。
- Permission blocked explanation。
- Tool progress event。

消息中不直接展示完整 trace。默认只展示必要摘要，点击后进入 Run Plan Drawer。

## 4. Inline AI Surface

Inline AI 是主助手的轻量入口，不是第二套 AI 系统。

触发方式：

- 选中文本后的 AI 菜单。
- slash command。
- 右键菜单。
- 文档标题、目录、反链、引用处的上下文动作。

规则：

- Inline AI 直接生成 `AgentIntent` hint。
- 结果优先以 patch preview 展示。
- 复杂任务自动升级到 Assistant Panel，并保留上下文。
- 不在 inline surface 展示复杂 settings。

常见动作：

- 改写
- 扩写
- 总结
- 翻译
- 简化
- 增加引用
- 检查事实
- 提取为新笔记
- 生成标题
- 修复链接

## 5. Run Plan Drawer

Run Plan Drawer 是用户理解 Agent 行为的核心界面。

默认折叠，只显示一句摘要：

```text
将使用 Writer 模型，读取当前笔记和 3 条相关证据，调用 1 个写入工具，需要确认补丁。
```

展开后分区：

- **Intent**：识别到的任务、置信度、来源。
- **Context**：当前笔记、选区、检索范围、附件。
- **Model**：slot、provider、model、选择原因、fallback。
- **Persona**：identity、style、task overlay、skill overlay。
- **Skills**：激活 skill、匹配原因、注入摘要、阻断项。
- **Tools**：可用工具、将调用工具、确认状态。
- **Permissions**：自动允许、需确认、已阻断。
- **Progress**：当前步骤、耗时、token、错误。

敏感数据只展示摘要，不展示正文、base64、key、剪贴板内容。

## 6. Confirmation Center

所有确认体验走统一模式。

确认类型：

- Markdown patch。
- 新建笔记。
- 移动/重命名。
- 删除到回收站。
- 导入外部文件。
- 导出文件。
- 网页下载。
- skill 安装/卸载/启用。
- sandbox script execution。
- shell/git 高级操作。
- clipboard read。
- browser control。

确认弹窗必须包含：

- 动作名称。
- 权限名称。
- 作用域。
- 风险等级。
- 将读取或修改的路径、域名、工具摘要。
- 可撤销方式。
- approve / reject / modify。

Markdown 写入确认必须显示 diff preview：

- before / after。
- 影响范围。
- 目标文件。
- base content hash。
- 是否可回滚。

对于高风险操作，不提供“永久允许”作为默认按钮。

## 7. Agent Settings

设置页新增 AI Agent 分组，包含四个主要 tabs。

### Models

展示：

- Provider card。
- API Key 状态。
- Base URL。
- Model ID。
- 能力标签。
- 连接测试。
- 视觉测试。
- 工具测试。
- 模型列表刷新状态。

路由区展示 capability slots：

- Fast
- Writer
- Reasoner
- Long Context
- Vision
- Agent Tools
- Local Private

用户可以手填 model id。未配置 key 时展示静态种子目录。

### Persona

展示单一 PromptProfile：

- display name。
- avatar。
- persona。
- writing style。
- language。
- custom rules。

明确说明：

- 人格影响表达和偏好。
- 人格不授予工具或权限。
- safety overlay 不可被人格覆盖。

### Skills

Skills 页面展示：

- installed / enabled。
- scope: global / vault。
- last matched。
- last used。
- activation score。
- requested tools。
- requested capabilities。
- unsupported capabilities。
- confirmation-required capabilities。
- resource status。
- sandbox/script status。
- Hermes compatibility mapping。

每个 skill 有诊断入口：

- 为什么本轮激活。
- 为什么未激活。
- 缺少哪些能力。
- 哪些能力被产品范围拒绝。
- 是否有 Markdown 工作台替代能力。

### Permissions

按权限域分组：

- Vault
- External Files
- Documents
- Web
- Skills
- Shell/Git
- Clipboard/Browser
- Secrets

每组展示：

- 默认策略。
- 已授权作用域。
- 最近使用。
- 撤销入口。
- 高风险能力开关。

## 8. 状态与文案

需要统一的状态语言：

- `Planning`：正在规划本轮任务。
- `Retrieving`：正在读取上下文。
- `Thinking`：模型正在生成。
- `Using tools`：正在调用工具。
- `Waiting for confirmation`：等待确认。
- `Blocked`：因权限、能力或配置阻断。
- `Degraded`：使用降级路径。
- `Completed`：完成。

阻断文案必须给出：

- 发生了什么。
- 为什么阻断。
- 用户能做什么。
- 是否有替代方案。

示例：

```text
此 skill 请求运行脚本，但当前未启用 sandbox script execution。你可以在 Skills 设置中为该 skill 单次授权，或改用不执行脚本的 Markdown 工作台能力。
```

## 9. 响应式与可访问性

布局要求：

- 侧栏宽度变化时，Header 不换成多行堆叠噪音。
- chips 可换行，但不能挤压输入框到不可用。
- Run Plan Drawer 在窄屏上变为覆盖层。
- 确认弹窗内容过长时内部滚动，操作按钮固定。
- Diff preview 在窄屏上默认单栏。

可访问性：

- 所有 icon button 有 tooltip 或 aria label。
- 键盘可完成发送、停止、确认、拒绝、打开 RunPlan。
- 风险等级不能只靠颜色表达。
- 错误和阻断信息可被屏幕阅读器读到。

## 10. 组件边界

建议组件边界：

- `UnifiedAssistantPanel`
- `AiComposer`
- `ContextChipBar`
- `AttachmentChipList`
- `AiMessageList`
- `RunPlanDrawer`
- `RunPlanSummary`
- `PermissionPreflightView`
- `ToolProgressTimeline`
- `SkillActivationView`
- `ModelRouteBadge`
- `PersonaLayerView`
- `AgentConfirmationDialog`
- `PatchDiffPreview`
- `AgentSettingsPanel`
- `ModelProviderCard`
- `CapabilityRouteSettings`
- `SkillDiagnosticsPanel`
- `PermissionSettingsPanel`
- `AgentAuditView`

`components/ui/` 只放 shadcn/ui 原语，不放 AI 业务逻辑。AI 业务组件放在 `components/ai/`。

## 11. 测试计划

- 单一入口：chat、inline、slash、context menu 都进入统一 pipeline。
- RunPlan：摘要、展开详情、阻断状态、降级状态都正确显示。
- Confirmation：patch、skill、权限、shell/git、clipboard/browser 确认都使用同一模式。
- Settings：模型、人设、skills、权限四类配置可独立操作。
- Skills：激活、未激活、阻断、Hermes 映射都可见。
- 权限：撤销后下一轮 preflight 重新阻断。
- 响应式：窄侧栏、宽侧栏、弹窗滚动、diff 单栏。
- 安全：UI 不渲染 API Key、图片 base64、剪贴板正文、敏感 shell 输出。

## 12. 验收标准

- 用户不需要理解底层场景，也能稳定使用 AI。
- 用户能在需要时看懂 Agent 为什么这么做。
- 用户能明确控制写入、联网、导入、命令、skill 权限。
- 模型路由、人设、skills、权限都有设置入口，但不会压垮日常使用。
- 复杂诊断存在，但默认折叠。
- 所有 UI 与 `docs/design-system.md` 和 Iris Rail 视觉方向一致。
