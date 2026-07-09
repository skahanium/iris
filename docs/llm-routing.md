# LLM 路由与连通性

## 配置存储

- **路由表**：`settings.llm_routing`（JSON，schema v4），含能力槽 `providerId` / `model` / `reasoning.mode`、`contextStrategy`（`hybrid` | `long_context`）、各厂商 `baseUrl` 覆盖、手动启用模型与模型级 `modelCapabilities` 覆盖。
- **模型注册表**：`llm_model_registry` 合并内置目录、供应商 `/models` 发现结果和用户手动模型；未知模型默认只进入文本候选，Vision / Long context / Reasoner 需要内置目录、专项验证或用户确认。
- **API Key**：系统凭据 `iris.llm.{provider_id}`（勿含 `/`，兼容 Windows），禁止写入设置文件或日志。

## 出厂默认

| 场景               | 模型                |
| ------------------ | ------------------- |
| 知识查阅、文稿学习 | `deepseek-v4-flash` |
| 文稿创作、学术研究 | `deepseek-v4-pro`   |

设置页按 **供应商连接 / 模型目录 / 能力路由** 分层：供应商级测试只检查凭据和端点；模型级验证会对指定模型发起文本或视觉探测；能力路由只展示已启用且满足该能力的模型。

## Reasoning / Thinking

- Fast / Writer / Reasoner / Long context 能力槽可配置 `关闭`、`自动`、`低`、`中`、`高`；Vision 不显示思考强度控件。
- 旧 `thinking: true` 路由读取时兼容为 `reasoning.mode = "auto"`；新配置保存结构化 `reasoning`，不依赖单一布尔值表达模型能力。
- `自动` 按能力槽解析：Fast / Writer 偏低强度或关闭，Reasoner / Long context 偏中强度；不支持原生参数的模型不会被强塞 thinking 字段。
- 自定义 OpenAI-compatible 模型默认不发送未知 provider-specific 参数；验证模型后 Iris 只记录安全元数据，如 adapter、control、visibility 和验证时间，不记录 prompt 或原始 reasoning。
- Agent task 诊断事件只记录 provider、model、capability slot、endpoint、reasoning mode / adapter / visibility、是否输出隔离和预算估算，不记录 API Key、笔记正文、网页正文或完整 prompt。
- 原始 chain-of-thought、`reasoning_content`、`<think>` / `<thinking>` / `<reasoning>` 标签内容不会作为普通 assistant 回复保存。标签风险模型会先走内部候选，清洗后再进入可见答案。

首批兼容目标：

| Provider / 模型族           | 默认策略                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------ |
| OpenAI reasoning models     | 明确支持时发送 reasoning effort；普通 OpenAI-compatible 不盲发                       |
| Anthropic extended thinking | 在 Anthropic Messages endpoint 写入 extended thinking budget，并保证预算小于输出上限 |
| Gemini thinking             | adapter 保留为独立 endpoint 能力；当前不会复用 OpenAI-compatible 请求体              |
| DeepSeek reasoner           | 消费 `reasoning_content`，多轮历史只回放最终 `content`                               |
| GLM 4.5+/5.x                | 支持 `thinking.reasoning_effort` 映射                                                |
| Qwen3                       | 使用临时 chat-template/tag 控制与隔离路径，支持 `/think` / `/no_think` 类模型        |
| Doubao / Volc Ark           | 默认安全关闭，后续由 catalog / probe / 用户覆盖启用                                  |
| MiniMax / MiniMax-M3        | 默认标签/元分析风险隔离，不把 M3 能力等同于 API 参数能力                             |
| MiMo                        | 以内置 catalog 为准，走 provider-specific static adapter                             |

**DeepSeek Base URL** 推荐 `https://api.deepseek.com`（无 `/v1` 后缀）。Iris 会自动请求 `/v1/chat/completions` 与 `/models`；若在设置里填写带 `/v1` 的地址也能兼容。LLM provider 仅支持 HTTPS 端点；Ollama / localhost HTTP 通道已移除，旧 `ollama` 路由会在读取时回退到 DeepSeek 默认模型。

## 安全约束

- `llm_config_get` 返回的 provider 列表不包含 `ollama`。
- `llm_config_set` 会拒绝任何 `http://` base URL，包括 localhost、127.0.0.1 和 IPv6 loopback。
- 通用 `settings_set` 不允许写入 `llm_routing`；LLM 路由必须通过 `llm_config_set` 保存，以保证 provider 与 HTTPS 校验始终生效。
- 自定义 provider 必须使用 HTTPS OpenAI-compatible endpoint，API Key 仅保存在 Iris 本地加密凭据 `iris.llm.{provider_id}`。

## 长上下文

- 模型能力见 Rust `llm/model_catalog.rs`（DeepSeek V4 等为 1M `context_window`）。
- `resolve_for_task_policy` / `resolve_capability_route` 计算 `input_budget` / `output_budget`；`packet_builder` 按预算缩放 Top-K。
- `long_context` 策略会优先注入当前笔记全文，检索包作补充。

## DeepSeek 前缀缓存纪律

1. 消息分层：静态人设与规则 → 动态环境 → Skills → 证据包 → 会话历史（可变内容在稳定层之后）。
2. 会话内不重写已发送的稳定层；新证据以追加消息进入历史。
3. 同会话保持 `model`、`temperature`、思考模式一致；勿混用 `deepseek-reasoner` 与 Flash 非思考。
4. 命中率：`usage.prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` 写入 `settings.llm_usage_last`，底栏 LLM 指示器 hover 可查看。

## IPC

- `llm_config_get` / `llm_config_set`
- `llm_config_test`（兼容旧入口，不记录 Key）
- `llm_config_test_provider`（供应商端点连通性）
- `llm_model_registry_refresh`（刷新供应商模型目录）
- `llm_model_validate`（按模型做文本 / 视觉验证）
- `llm_model_confirm_capability`（用户显式确认模型能力）
- `connectivity_status`（可选 `scene`）

## 底栏指示

- 两枚 **8px 圆点 + 简短文案**（LLM · 联网）并排成组，未就绪统一灰（`--status-inactive`），就绪分别为 emerald / sky token。
- **LLM**：emerald 就绪 / red 检测失败 / 灰 未配置或缺 Key。
- **联网**：底栏 sky 圆点开/关；开启后 **仅** AI 场景条底边一条质感蓝线；所有网页证据统一经 `WebEvidenceBroker` 收集为证据包。
- **主通道**：Broker 只选择已启用且显式映射 `web.search` / `web.fetch` 的 MCP provider；缺失时联网搜索不可启用。
- **模型供应商**：普通 LLM provider 不作为 web evidence backend，不提供厂商搜索配置 IPC。
- **深读（正文）**：用户明确给出 HTTPS URL 时仍通过 `web_search` 语义进入 Broker，由 MCP `web.fetch` 与 native readability provider 受控抓取正文；不暴露独立抓取工具。
- 对话模型不受检索通道影响，仍按能力槽路由。

## Web Evidence Provider Target Contract

- `web_search` is the only assistant-facing network switch. Search, URL deep-read, and page fetch evidence all enter `WebEvidenceBroker`; explicit URLs are passed as broker URLs rather than through a separate fetch tool.
- MCP providers are persisted only when explicitly configured as web evidence providers. Supported transport configs are `{"url":"https://...","allow_localhost_dev":false}` for HTTPS and `{"command":"...","args":[...]}` for stdio. Credential fields store Iris local encrypted credential service references only.
- Iris does not ship a native or vendor-specific search engine fallback for web evidence.
- Broker candidate ordering is MCP-provider only. Provider diagnostics expose mapping completeness and recent circuit state in Management Center.
- Ordinary evidence details show citation, title, safe URL/domain, retrieval reason, conflict markers, and excerpt. Provider ids, provider kind, raw result hashes, and extraction methods are audit/diagnostic metadata only.

## 本地检索加速

- 普通 AI Agent 检索通过 `search_hybrid` / `search_semantic` 进入 retrieval broker；默认构建保持 sqlite-vec 关闭，语义向量层未就绪时 `search_hybrid` 仍使用 FTS 等非向量层返回结果。
- 本地验证 sqlite-vec 时使用 `npm run dev:desktop:vec`；生产验证构建使用 `npm run tauri:build:vec`。这两个脚本都会向 Tauri/Rust 传入 `--features sqlite-vec`。
- sqlite-vec 仅加速普通笔记索引。`.classified/` 文档不得写入主 SQLite FTS、`chunk_embeddings` 或 `vec_*` 表；涉密 AI 继续使用独立的内存检索索引。
- 启动和 `search_reindex` 后会记录 `chunk_embeddings` 与 `vec_chunks` 的非敏感计数差异；日志只包含数量，不包含路径、标题、正文、摘要或涉密标识。
- 性能对比入口在 `cargo bench --manifest-path src-tauri/Cargo.toml --bench ai_benchmarks retrieval_hybrid_synthetic_corpus`，用于观察 1k / 10k / 50k 合成语料下的检索延迟分布。
