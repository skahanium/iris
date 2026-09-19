# V05 DeepSeek-Flash 原生搜索实网探针（2026-09-20）

本文件是 [2026-09-19 V05 逐端点实网验证窗口](./2026-09-19-v05-live-window.md) 授权后的**执行记录**，不是关闭证明。

- 承接：`G03`／`Q17`／`K12`／`D04`
- 本轮只跑 **DeepSeek-Flash** 原生搜索协议探针与适配器 live（诊断调用，非全矩阵）
- 主对话目录族仍是 `OpenAiCompatibleChatCompletions`（`https://api.deepseek.com`）
- **不关闭** `G02`、`G03`、`Q05`、`Q17`、`D04`；不把 `C20`／`C11`／`K12` 标为 verification passed
- 不含 API Key、笔记正文、私有推理；检索正文只记长度与结构类型

## 一、本轮批准对照

| # | 窗口要求 | 本轮事实 |
| - | -------- | -------- |
| 1 | 端点范围 | DeepSeek-Flash 原生搜索协议探针；模型出站名 `deepseek-flash`，目录 id `deepseek-v4-flash` |
| 2 | 费用上限 | 用户声明本轮 DeepSeek 无上限 |
| 3 | 凭据 | 本地加密存储 `iris.llm.deepseek` 密文存在且可解密；值未写入本文件、日志或仓库 |
| 4 | 原生搜索厂商 | DeepSeek-Flash；**不得**套用 MiniMax-M3 的 Responses `/v1/responses` 协议 |

## 二、与 MiniMax-M3 的协议差别（文档 + live）

官方现页（2026-09-20 抓取 `api-docs.deepseek.com`）：

| 面 | DeepSeek 现页 | MiniMax-M3 live（对照） |
| -- | ------------- | ---------------------- |
| Chat Completions 工具 | 仅 `function` | 主对话仍是 Chat Completions |
| Responses | `POST https://api.deepseek.com/responses`（`/v1/responses` live 也 200）；`web_search` **忽略**；`tool_choice` 无 `{type:web_search}` | `POST https://api.minimaxi.com/v1/responses` + `{type:web_search}` **有** `web_search_call` |
| Anthropic | `https://api.deepseek.com/anthropic`；Claude Code 文档声称服务端 Web Search；消息块支持回传 `server_tool_use`／`web_search_tool_result` | 同主机 Anthropic `web_search_20250305` 为 200 纯文本，**无**检索 |
| 思考 | 默认开启；子请求需 `thinking.disabled` / `reasoning.effort=none` | Responses 无此默认思考字段 |
| 模型名 | 出站 `deepseek-flash`；遗留 `deepseek-v4-flash` 仍接受 | 出站 `MiniMax-M3` |

搜索引擎与旧归档仍可能显示 DeepSeek Responses 服务端 `web_search`。**以现页 + 本轮 live 为准**，不得按旧索引接线。

## 三、协议矩阵（结构摘要，无正文）

诊断提示：`What is the weather in Shanghai?`；`model=deepseek-flash`；`stream=false`。

| 名称 | 路径 | 工具声明 | HTTP | 耗时 | 结构 | 检索凭据 |
| ---- | ---- | -------- | ---- | ---- | ---- | -------- |
| `responses_root_auto` | `/responses` | `{type:web_search}` + `tool_choice=auto` + `reasoning.effort=none` | 200 | ~0.1–0.2s | `message,output_text,text,web_search`（工具回声） | **无** `web_search_call` |
| `responses_root_forced` | `/responses` | `{type:web_search}` + `tool_choice={type:web_search}` | 200 | ~0.1s | 同上 | **无**（forced 被静默忽略） |
| `responses_root_required` | `/responses` | `{type:web_search}` + `tool_choice=required` | 200 | ~0.1s | 同上 | **无** |
| `responses_root_web_search_2025_08_26` | `/responses` | `{type:web_search_2025_08_26}` | 200 | ~0.1s | 工具回声 `web_search_2025_08_26` | **无** |
| `responses_v1_auto` | `/v1/responses` | 同 auto | 200 | ~0.1s | 同工具回声 | **无**（路径可通，仍无检索） |
| `chat_v1_web_search_tool` | `/v1/chat/completions` | `{type:web_search}` | **422** | ~0.06s | `invalid_request_error` | 无 |
| `anthropic_web_search_20250305` | `/anthropic/v1/messages` | `web_search_20250305` + `name=web_search` | 200 | 一次跳过（纯 `text`）；一次命中 | 命中：`server_tool_use`,`web_search_tool_result`,`web_search_result` | **有**（Anthropic 块，不是 `url_citation`） |
| `anthropic_type_web_search` | `/anthropic/v1/messages` | `{type:web_search,name:web_search}` | **422** | ~0.1s | `invalid_request_error` | 无 |

归类：

- Responses 200 工具回声 = **协议／结果不足**，不是传输失败，也不得写成「模型不支持」。
- Chat Completions 422 = 该端点不接受内置 `web_search` 工具类型。
- Anthropic `web_search_20250305` = **可承载原生搜索**；认证头是 `x-api-key`（不是 MiniMax Responses 的 Bearer + `/v1/responses`）。模型仍可能跳过工具，适配器对 200 纯文本重试一次。

## 四、适配器 live

`live_deepseek_flash_adapter_returns_https_citations`：K12 隔离子请求走 Anthropic Messages，主对话仍是 Chat Completions。HTTP 200 且解析出 HTTPS `web_search_result` URL 后视为检索凭据。本轮该测试 **3/3** 通过（约 4.8s / 6.7s / 5.8s）。

生产登记：`production_native_search_adapter_count() == 2`（MiniMax-M3 + DeepSeek-Flash）。匹配目录 id / API 名，**不**匹配品牌 `deepseek` 或 `deepseek-v4-pro`。

## 五、适用范围结论

- **不是**「DeepSeek 端点机械与 live 均通过」。
- **不是** `G03`／`K12`／`D04` 关闭。
- 不得把 DeepSeek Responses 200 或 MiniMax 的 `{type:web_search}` 当成 DeepSeek 已搜。
- 流式、截断、混合续轮、K06 入账、`streaming.rs` 搜索事件、斜杠命令 `native_endpoint: None` 仍缺。
