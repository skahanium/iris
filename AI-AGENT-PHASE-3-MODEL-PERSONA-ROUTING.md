# Phase 3: Model + Persona Routing

> 状态：规划草案  
> 目标：统一模型能力路由与人格注入，让模型、人格、意图协作但不越权。

## 1. 目标

Phase 3 解决三个问题：

- 模型配置不能只高标准适配单一厂商，需要通过 Provider Adapter + Capability Registry 兼容多模型。
- 人格设置不能分散在 UI 身份、PromptProfile、Agent 状态里重复注入。
- 模型路由、人格、任务意图不能互相覆盖或制造隐藏行为。

## 2. 模型能力路由

采用 capability slot：

- `fast`
- `writer`
- `reasoner`
- `long_context`
- `vision`
- `agent_tools`
- `embedding`
- `reranker`
- `local_private`

每个 provider/model 注册能力：

- supports vision
- supports tools
- supports streaming
- supports reasoning
- context window
- max output
- endpoint family
- credential service
- probe strategy

默认 provider 范围：

- DeepSeek
- OpenAI
- Anthropic
- GLM/Zhipu
- Kimi
- 豆包/火山方舟
- Ollama
- 自定义 OpenAI-compatible
- MiMo experimental preset

## 3. Adapter 边界

内部统一消息表达：

- text part
- image part
- tool calls
- reasoning metadata
- streaming chunks

Adapter 负责序列化到：

- OpenAI-compatible Chat Completions
- Anthropic Messages
- Ollama Chat

Responses API 先只在类型中预留，不作为本阶段必交付。

## 4. 路由规则

路由输入：

- `AgentIntent`
- context size
- image attachments
- tool need
- reasoning need
- privacy preference
- user slot settings
- provider health

路由输出：

- selected slot
- selected provider/model
- fallback chain
- reason
- probe status

示例：

- 带图请求 -> `vision`
- 大型文档分析 -> `long_context`
- 工具循环 -> `agent_tools`
- 普通问答 -> `fast`
- 写作润色 -> `writer`
- 论证/研究 -> `reasoner`

## 5. 人格分层

人格分层为：

- `identity`：助手称呼、语气基调。
- `style`：写作风格、语言偏好。
- `task_overlay`：当前任务需要的临时行为。
- `skill_overlay`：skill 注入的任务指导。
- `safety_overlay`：安全、权限、数据边界。

注入顺序固定，避免重复：

```text
safety_overlay
  -> identity
  -> style
  -> task_overlay
  -> skill_overlay
```

规则：

- `PromptProfile` 是人格唯一数据源。
- UI identity 只读展示，不另存一份可冲突配置。
- skill 可以提供 task guidance，但不能改写用户人格主配置。
- safety overlay 优先级最高，不能被 persona 或 skill 覆盖。

## 6. 设置 UI

模型设置：

- Provider 卡片展示 key 状态、base URL、模型、能力标签、连接测试、视觉测试、工具测试。
- 路由区展示 capability slots。
- 无 key 时展示内置静态模型目录。
- 有 key 后尝试刷新模型列表。
- 始终允许手填 model id。

人格设置：

- 保留单一 `PromptProfile` 面板。
- 展示身份、语言、写作风格、自定义规则。
- 显示“会影响表达，不会影响权限”的说明。

## 7. 测试计划

- Adapter contract：OpenAI-compatible、Anthropic、Ollama 的文本、工具、流式、视觉 body 序列化。
- 路由测试：vision、long_context、agent_tools、writer、reasoner 选择正确 slot。
- fallback 测试：主模型不可用时回退到配置链。
- 人格测试：不重复注入；skill overlay 不覆盖 safety overlay；PromptProfile 是唯一来源。
- 安全测试：API Key、图片 base64、用户笔记正文不进入日志。
- UI 测试：能力标签、slot 配置、连接测试、视觉测试、工具测试可见。

## 8. 验收标准

- 模型路由按能力解释，不按硬编码厂商解释。
- 带图请求不会被路由到无视觉模型。
- 工具请求不会被路由到无 tool capability 模型，除非走降级非工具模式。
- 人格表现稳定，不再多处配置冲突。
- 权限不受 persona 或模型选择影响。
