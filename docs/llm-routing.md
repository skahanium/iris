# LLM 路由与连通性

## 配置存储

- **路由表**：`settings.llm_routing`（JSON），含能力槽 `providerId` / `model`、`contextStrategy`（`hybrid` | `long_context`）、各厂商 `baseUrl` 覆盖与手动启用模型。
- **模型注册表**：`llm_model_registry` 合并内置目录、供应商 `/models` 发现结果和用户手动模型；未知模型默认只进入文本候选，Vision / Long context / Reasoner 需要内置目录、专项验证或用户确认。
- **API Key**：系统凭据 `iris.llm.{provider_id}`（勿含 `/`，兼容 Windows），禁止写入设置文件或日志。

## 出厂默认

| 场景               | 模型                |
| ------------------ | ------------------- |
| 知识查阅、文稿学习 | `deepseek-v4-flash` |
| 文稿创作、学术研究 | `deepseek-v4-pro`   |

设置页按 **供应商连接 / 模型目录 / 能力路由** 分层：供应商级测试只检查凭据和端点；模型级验证会对指定模型发起文本或视觉探测；能力路由只展示已启用且满足该能力的模型。

**DeepSeek Base URL** 推荐 `https://api.deepseek.com`（无 `/v1` 后缀）。Iris 会自动请求 `/v1/chat/completions` 与 `/models`；若在设置里填写带 `/v1` 的地址也能兼容。LLM provider 仅支持 HTTPS 端点；Ollama / localhost HTTP 通道已移除，旧 `ollama` 路由会在读取时回退到 DeepSeek 默认模型。

## 安全约束

- `llm_config_get` 返回的 provider 列表不包含 `ollama`。
- `llm_config_set` 会拒绝任何 `http://` base URL，包括 localhost、127.0.0.1 和 IPv6 loopback。
- 通用 `settings_set` 不允许写入 `llm_routing`；LLM 路由必须通过 `llm_config_set` 保存，以保证 provider 与 HTTPS 校验始终生效。
- 自定义 provider 必须使用 HTTPS OpenAI-compatible endpoint，API Key 仅保存在系统凭据管理器 `iris.llm.{provider_id}`。

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

- 两枚 **8px 圆点 + 简短文案**（LLM · 联网）并排成组，未就绪统一灰（`--status-inactive`），就绪分别为 emerald / sky token
- **LLM**：emerald 就绪 / red 检测失败 / 灰 未配置或缺 Key
- **联网**：底栏 sky 圆点开/关；四场景共用。开启后 **仅** AI 场景条底边一条质感蓝线；发送前自动注入网页摘要
  - **主通道**：MiniMax Token Plan `POST /v1/coding_plan/search`（国内默认 `https://api.minimaxi.com`，Key 为 `iris.minimax`；可选请求体字段 `model`，设置页「检索模型名称」）
  - **API Host 约束**：仅接受干净 HTTPS origin（如 `https://api.minimaxi.com`）；拒绝 HTTP、userinfo、query、fragment、空 host 和额外 path。
  - **降级**：无 Key、API 失败或设置强制 `duckduckgo` 时使用 DuckDuckGo HTML
  - **深读（正文）**：助手工具 `fetch_web_page` 对单个 HTTPS URL 受控抓取正文（需用户确认、每轮 1～2 次）；与 Token Plan 搜索（仅摘要）互补，非 MCP 运行时
  - 对话模型不受检索通道影响，仍为 DeepSeek 四场景路由

## MiniMax 联网检索 IPC

- `minimax_config_get` / `minimax_config_set`（Host、`minimax_search_model`、`web_search_backend`：`auto` | `minimax` | `duckduckgo`；Host 保存时会规范化并强制 HTTPS origin）
- `minimax_config_test`（极简查询探测 Key，不记录 Key）
- Key 读写：`credential_set` / `credential_has` / `credential_delete`，服务 ID `iris.minimax`
