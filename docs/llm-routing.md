# LLM 路由与连通性

## 配置存储

- **路由表**：`settings.llm_routing`（JSON），含四场景 `providerId` / `model`、`contextStrategy`（`hybrid` | `long_context`）、各厂商 `baseUrl` 覆盖。
- **API Key**：系统凭据 `iris.llm.{provider_id}`（勿含 `/`，兼容 Windows），禁止写入设置文件或日志。

## 出厂默认

| 场景 | 模型 |
|------|------|
| 知识查阅、文稿学习 | `deepseek-v4-flash` |
| 文稿创作、学术研究 | `deepseek-v4-pro` |

设置页可一键 **DeepSeek 推荐** 恢复上表。

**DeepSeek Base URL** 推荐 `https://api.deepseek.com`（无 `/v1` 后缀）。Iris 会自动请求 `/v1/chat/completions` 与 `/models`；若在设置里填写带 `/v1` 的地址也能兼容。

## 长上下文

- 模型能力见 Rust `llm/model_catalog.rs`（DeepSeek V4 等为 1M `context_window`）。
- `resolve_for_scene` 计算 `input_budget` / `output_budget`；`packet_builder` 按预算缩放 Top-K。
- `long_context` 策略会优先注入当前笔记全文，检索包作补充。

## DeepSeek 前缀缓存纪律

1. 消息分层：静态人设 → 用户规则 → 证据包 → 会话历史（可变内容在历史尾部）。
2. 会话内不重写已发送的稳定层；新证据以追加消息进入历史。
3. 同会话保持 `model`、`temperature`、思考模式一致；勿混用 `deepseek-reasoner` 与 Flash 非思考。
4. 命中率：`usage.prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` 写入 `settings.llm_usage_last`，底栏 LLM 指示器 hover 可查看。

## IPC

- `llm_config_get` / `llm_config_set`
- `llm_config_apply_deepseek_defaults`
- `llm_config_test`（`GET /models`，不记录 Key）
- `connectivity_status`（可选 `scene`）

## 底栏指示

- **LLM**：emerald 就绪 / amber 缺 Key / 灰 配置异常
- **搜索 API**：teal 已配置 Bing / 灰 DuckDuckGo 降级
- **联网搜索**（sky）：本次生成是否启用联网，与 Bing 是否配置无关
