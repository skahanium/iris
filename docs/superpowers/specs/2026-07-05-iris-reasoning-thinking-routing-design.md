---
title: Iris Reasoning / Thinking 能力槽路由设计
created: 2026-07-05
scope: llm-routing, model-catalog, model-registry, ai-harness, gateway, settings-ui
status: ready-for-planning
---

# Iris Reasoning / Thinking 能力槽路由设计

## 背景

Iris 当前已经有能力槽路由和模型目录，但 Reasoning / Thinking 只用一个 `thinking?: boolean` 表达。这个抽象过窄：

- 不同厂商的思考接口不统一。OpenAI、Anthropic、Gemini、DeepSeek、GLM、Qwen 等分别使用 effort、budget、`reasoning_content`、`thinking` 对象、chat template 或标签流。
- 自定义 OpenAI-compatible 模型验证通过后，Iris 仍不知道它是否支持原生思考参数、是否只会在正文里输出 `<think>`。
- 当前 Harness 清理逻辑只覆盖部分标签，导致 MiniMax-M3 一类模型可能把内部分析直接作为最终回复保存和展示。
- 设置页能力槽右侧有空间，但缺少对“思考模式”的可见、可控表达。

本设计把 Reasoning / Thinking 做成能力槽路由的一等配置，同时保持 Iris 的 Agent 表达自然，不把回复变成僵硬模板。

## 目标

- 在 Fast / Writer / Reasoner / Long context 能力槽中配置思考模式：`关闭`、`自动`、`低`、`中`、`高`。
- Vision 暂不开放思考强度控件，避免视觉输入、预算和 provider 参数交叉复杂化。
- 支持主流厂商差异化适配：OpenAI、Anthropic、Gemini、DeepSeek、GLM、Qwen、Doubao、MiniMax、MiMo，以及自定义 OpenAI-compatible。
- 自动识别已知模型能力，验证自定义模型时进行低成本能力探测，并允许用户手动覆盖。
- 隔离所有内部思考、标签思考和元分析，不把 chain-of-thought、`<think>`、`I should...` 这类内容展示或写入普通会话历史。
- 不新增数据库表，不新增 Tauri 命令，不改变 API Key 存储原则。

## 非目标

- 不重新设计 RAG 检索、证据包、工具调度或 Persona 全体系。
- 不把原始 chain-of-thought 做成用户可见功能。
- 不承诺 base URL 能可靠发现所有 reasoning 能力；base URL 只能作为启发式信号。
- 不强行给所有 OpenAI-compatible 端点发送未知 thinking 参数。

## 用户体验

能力槽路由保持现有五行布局：

- Fast：短问答、轻量检索、默认对话。
- Writer：改写、续写、章节与文档写作。
- Reasoner：研究、引用核查、复杂论证。
- Long context：长文档与大上下文分析。
- Vision：图片输入与视觉问答。

非 Vision 行在 provider/model 右侧增加 `思考模式` 下拉框：

| 状态           | UI 行为                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| 模型不支持思考 | 下拉框禁用，显示“不支持”                                                |
| 只支持开关     | 允许 `关闭` / `自动`                                                    |
| 支持强度       | 允许 `关闭` / `自动` / `低` / `中` / `高`                               |
| 能力未知       | 默认 `自动`，但实际不发送原生参数，只开启输出隔离；用户可在模型目录覆盖 |

默认策略：

- Fast：`自动` 解析为关闭或低强度，优先速度和简洁。
- Writer：`自动` 解析为关闭或低强度，避免文风被过度推理稀释。
- Reasoner：`自动` 解析为中强度，适合研究、核查、复杂论证。
- Long context：`自动` 解析为中强度，但先受上下文预算约束。
- Vision：不显示控件，保留普通视觉路由。

## 路由配置

`settings.llm_routing` 继续作为能力槽路由的唯一持久化入口。`SlotRoute` 从旧布尔值平滑升级：

```ts
type ReasoningMode = "off" | "auto" | "low" | "medium" | "high";

interface SlotRoute {
  providerId: string;
  model: string;
  thinking?: boolean; // 旧配置兼容读取，保存时不再新增
  reasoning?: {
    mode: ReasoningMode;
  };
}
```

兼容规则：

- 旧 `thinking: true` 读取为 `reasoning.mode = "auto"`。
- 旧 `thinking: false` 或缺失读取为 `reasoning.mode = "off"`，但新建路由默认可用 `auto`。
- 保存新配置时保留 `reasoning`，不主动写回旧 `thinking`。
- 真实任务发送路径只使用解析后的 `ResolvedReasoningRequest`，避免 UI 字段泄漏到 gateway。

Provider override 可在现有 JSON 内扩展模型级覆盖，不新增 DB 表：

```ts
interface ProviderOverride {
  baseUrl: string | null;
  label?: string | null;
  defaultModel?: string | null;
  enabledModels?: string[] | null;
  modelCapabilities?: Record<string, ModelCapabilityOverride>;
}

interface ModelCapabilityOverride {
  reasoningAdapter?: ReasoningAdapter;
  reasoningControl?: "none" | "switch" | "effort" | "budget";
  reasoningVisibility?: "hidden_channel" | "content_tag" | "plain_content_risk";
  userVerifiedAt?: string | null;
  probeVerifiedAt?: string | null;
}
```

覆盖优先级：

1. 用户显式覆盖。
2. live probe 结果。
3. 内置 catalog。
4. provider/base URL 启发式。
5. 安全默认：不发送原生思考参数，只做输出隔离。

## 能力模型

后端内部使用统一能力描述，不把厂商参数散落在 Harness 和 UI 中：

```rust
pub enum ReasoningMode {
    Off,
    Auto,
    Low,
    Medium,
    High,
}

pub enum ReasoningAdapter {
    None,
    OpenAiResponses,
    AnthropicExtendedThinking,
    GeminiThinkingConfig,
    DeepSeekReasoningContent,
    GlmThinking,
    QwenChatTemplate,
    OpenAiCompatibleTagStream,
    ProviderSpecificStatic,
}

pub enum ReasoningControl {
    None,
    Switch,
    Effort,
    Budget,
}

pub enum ReasoningVisibility {
    HiddenChannel,
    ContentTag,
    PlainContentRisk,
}
```

`ResolvedLlmConfig` 增加内部字段：

```rust
pub struct ResolvedReasoningRequest {
    pub mode: ReasoningMode,
    pub adapter: ReasoningAdapter,
    pub control: ReasoningControl,
    pub visibility: ReasoningVisibility,
    pub requested: bool,
    pub isolate_output: bool,
}
```

旧 `thinking: bool` 可保留为派生字段，直到调用点全部迁移完成。

## 主流厂商兼容矩阵

| Provider / 模型族                  | 默认 adapter                          | 强度控制                 | 输出隔离                 | 说明                                                                                                 |
| ---------------------------------- | ------------------------------------- | ------------------------ | ------------------------ | ---------------------------------------------------------------------------------------------------- |
| OpenAI reasoning models            | `OpenAiResponses`                     | effort                   | hidden channel           | 仅对 catalog/override 明确支持的模型发送 reasoning effort；普通 GPT-4o 类模型不发送。                |
| Anthropic Claude extended thinking | `AnthropicExtendedThinking`           | budget                   | hidden channel           | 映射为 thinking budget；需要校验输出预算，避免思考挤占最终答案。                                     |
| Google Gemini                      | `GeminiThinkingConfig`                | budget/switch            | hidden channel           | 映射到 Gemini thinking config；不假设 OpenAI-compatible 请求体可用。                                 |
| DeepSeek reasoner                  | `DeepSeekReasoningContent`            | switch                   | hidden channel           | 接收 `reasoning_content`，普通多轮历史只回放最终 `content`。                                         |
| GLM 4.5+/5.x                       | `GlmThinking`                         | effort                   | hidden channel           | 按 BigModel 文档使用 `thinking` 与 `reasoning_effort`；低/中可按官方规则映射。                       |
| Qwen3 hybrid / thinking            | `QwenChatTemplate`                    | switch/budget by runtime | content tag              | 本地/兼容端点默认用 `/think`、`/no_think` 或 chat template；DashScope 若探测确认再走 provider 参数。 |
| Doubao / Volc Ark                  | `OpenAiCompatibleTagStream` 或 `None` | probe only               | content tag              | 默认不发送未知 thinking 参数；如后续官方参数可用，以 catalog/override 启用。                         |
| MiniMax / MiniMax-M3               | `OpenAiCompatibleTagStream`           | probe only               | content tag / plain risk | 优先解决 `<think>`、英文元分析泄漏；不把 M3 论文能力等同于 API 参数能力。                            |
| MiMo                               | `ProviderSpecificStatic`              | switch                   | content tag              | 以 Iris 内置 catalog 为准；发送参数前由 adapter 明确控制，不再复用全局 `{ thinking: ... }`。         |
| 自定义 OpenAI-compatible           | `OpenAiCompatibleTagStream` 或 `None` | probe/override           | content tag              | 验证模型时探测；无法确认则只做隔离，不发送原生参数。                                                 |

兼容原则：

- “模型会推理”和“API 支持可配置思考参数”是两回事。
- “返回 `reasoning_content`”和“正文里输出 `<think>`”也是两回事。
- 任何 provider-specific 参数必须由 adapter 显式生成，不允许全局硬塞。

## Gateway 适配

`GatewayRequest` 从 `thinking: bool` 迁移到 `reasoning: ResolvedReasoningRequest`。

请求体构建规则：

- `None`：不写任何 reasoning/thinking 参数。
- `OpenAiResponses`：只在 endpoint/model 明确支持时写 OpenAI reasoning effort；chat completions 不盲写。
- `AnthropicExtendedThinking`：写 Anthropic extended thinking 参数，并确保 budget 小于输出上限。
- `GeminiThinkingConfig`：写 Gemini thinking config，不复用 OpenAI body。
- `DeepSeekReasoningContent`：不额外写通用 thinking 参数，消费流式/非流式 `reasoning_content`。
- `GlmThinking`：写 `thinking` 对象和 `reasoning_effort`，按 mode 映射官方枚举。
- `QwenChatTemplate`：用模板指令控制 `/think`、`/no_think`，并把 `<think>` 当作内部思考。
- `OpenAiCompatibleTagStream`：不发参数，只启用标签隔离和最终清洗。
- `ProviderSpecificStatic`：仅用于 Iris 明确内置的 provider，如 MiMo；实现必须有单测覆盖。

## Harness 与输出隔离

输出隔离分三层：

1. **结构化隐藏通道**：`reasoning_content`、Anthropic/Gemini/OpenAI 的原生 reasoning/thought 字段只进入内部 trace，不进入 visible stream。
2. **标签清洗**：支持 `<think>`、`<thinking>`、`<reasoning>`，大小写不敏感，允许未闭合标签，支持流式跨 chunk。
3. **元分析清洗**：如果最终答案开头出现 “The user is...”、“I should...”、“当前任务侧重...”、“The persona is...” 等内部策略段落，清洗到第一个用户可见回答起点。

流式策略：

- `HiddenChannel`：可继续流式展示普通 content。
- `ContentTag` / `PlainContentRisk`：先用 internal candidate 聚合并清洗，再作为 visible answer 输出，避免先漏后删。
- 清洗后的 assistant 回复才写入 `session_messages`。
- 原始 reasoning 不进入普通会话历史；诊断只记录 provider、adapter、mode、token 数和是否截断，不记录正文。

Prompt 层增加硬约束：

- 不在最终答案里展示内部分析、任务策略、人设说明、工具选择理由。
- 不用固定模板约束 Iris 的语气；允许自然、温暖、简洁的中文表达。

## 模型验证与探测

复用现有 `llm_model_validate`，不新增 Tauri 命令。文本验证成功后追加低成本 reasoning probe：

- 对已知 provider/model：按 catalog 直接给出能力，不额外发高风险参数。
- 对自定义 OpenAI-compatible：优先无参短问答，观察是否有 `reasoning_content` 或 `<think>`；再按用户选择或安全白名单尝试 provider-specific 参数。
- 如果端点对未知参数报错，记录 adapter 为 `None` 或 `OpenAiCompatibleTagStream`，不影响基础文本验证通过。
- 探测请求不得包含用户笔记、会话历史或敏感内容。

## 安全与隐私

- API Key 仍只存 OS credential manager。
- 不把 API Key、网页正文、笔记正文、完整 prompt、原始 reasoning 写入日志或数据库。
- ai trace / agent task event 只记录安全元数据：provider、model、slot、reasoning mode、adapter、是否隔离、估算 token。
- 对 DeepSeek-like 多轮，不把上轮 `reasoning_content` 回放给 provider。

## 验收标准

- MiniMax-M3 输入“你好?”时，不显示英文内部分析，不保存 `<think>` 或 meta-analysis。
- 支持 GLM 的 `thinking.reasoning_effort` 映射，并有单测覆盖。
- 支持 Qwen `<think>` / `/think` / `/no_think` 类路径的隔离和关闭。
- 不支持 reasoning 的模型不会收到任何 thinking 参数。
- 自定义 provider 即使 reasoning 探测失败，只要文本验证成功，仍可正常用于能力槽。
- Fast / Writer / Reasoner / Long context 的思考模式 UI 与保存/重载一致，Vision 不显示该控件。

## 参考

- OpenAI reasoning guide: https://platform.openai.com/docs/guides/reasoning
- Anthropic extended thinking: https://docs.anthropic.com/en/docs/build-with-claude/extended-thinking
- Gemini thinking: https://ai.google.dev/gemini-api/docs/thinking
- DeepSeek reasoning model: https://api-docs.deepseek.com/guides/reasoning_model
- BigModel GLM chat completion: https://docs.bigmodel.cn/api-reference/模型-api/对话补全
- Qwen thinking mode: https://qwen.readthedocs.io/en/latest/inference/transformers.html
- Volc Ark chat API: https://www.volcengine.com/docs/82379/1494384
