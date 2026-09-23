# V05 DeepSeek-Flash 原生搜索 live（2026-09-23 1A 窄窗口）

本文件是 [2026-09-19 V05 逐端点实网验证窗口](./2026-09-19-v05-live-window.md) **1A 窄窗口**书面批准后的执行记录，不是关闭证明。

- 承接：`G03`／`Q17`／`K12`／`D04`
- 本窗口跑：`live_deepseek_flash_native_search_protocol_probe`、`live_deepseek_flash_adapter_returns_https_citations`
- 先前因本机无 `iris.llm.deepseek` 记跳过；用户补配 Key 后于同日补跑。**两条均通过**（协议探针断言仅为「至少一次 HTTP status」，适配器断言为检索凭据 + HTTPS 候选）
- **不关闭** `G02`、`G03`、`Q17`、`D04`；不把 `C10`／`C11`／`K12` 标为 `verification.passed`
- 不把 `npm run agent:eval:live` 当 V05；生产目录仍无「带工具的 Responses」主对话端点
- 不含 API Key、笔记正文、私有推理；引用只记去重主机名

## 一、本轮批准对照

| #   | 窗口要求     | 本轮事实                                                                                 |
| --- | ------------ | ---------------------------------------------------------------------------------------- |
| 1   | 端点范围     | DeepSeek-Flash；模型出站名 `deepseek-flash`，目录 id `deepseek-v4-flash`                 |
| 2   | 费用上限     | 本窗口不另设数字上限；协议探针 8 次出站 + 适配器 live 1 次，未缩范围重跑                 |
| 3   | 凭据         | `.iris-dev/app-data` 中 `iris.llm.deepseek` 密文存在且可解密；值未写入本文件、日志或仓库 |
| 4   | 原生搜索厂商 | DeepSeek-Flash；**不得**套用 MiniMax-M3 的 Responses `/v1/responses` 协议                |

## 二、凭据探查（未解密、未打印密文）

- 配置目录 `~/Library/Application Support/Iris/config` 存在 `master.key`（未读取内容）。
- `~/iris/.iris-dev/app-data`：`credential.configured.iris.llm.deepseek` 有标记；按 `service:api_key` 哈希核验证件文件存在。
- 生产目录 `~/Library/Application Support/com.iris.notes/app-data`：**仍无** `iris.llm.deepseek` 文件。测试 `install_deepseek_live_dirs` 先试 `.iris-dev/app-data`，本轮走该路径。

## 三、命令与结果

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib live_deepseek_flash_native_search_protocol_probe -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib live_deepseek_flash_adapter_returns_https_citations -- --ignored --nocapture
```

| 探针                                                  | 结果                         | 墙钟   |
| ----------------------------------------------------- | ---------------------------- | ------ |
| `live_deepseek_flash_native_search_protocol_probe`    | **通过**（至少一次 HTTP）    | 10.15s |
| `live_deepseek_flash_adapter_returns_https_citations` | **通过**（检索凭据 + HTTPS） | 5.74s  |

诊断提示：`What is the weather in Shanghai?`；`model=deepseek-flash`；`stream=false`。主机 `api.deepseek.com`。

### 协议探针矩阵（结构摘要，无正文）

| 名称                                   | 路径                     | HTTP    | 耗时  | 结构摘要                                                                               | 检索凭据                                    |
| -------------------------------------- | ------------------------ | ------- | ----- | -------------------------------------------------------------------------------------- | ------------------------------------------- |
| `responses_root_auto`                  | `/responses`             | 200     | 141ms | `message,output_text,text,web_search`                                                  | **无** `web_search_call`；https_citations=0 |
| `responses_root_forced`                | `/responses`             | 200     | 80ms  | 同上                                                                                   | **无**                                      |
| `responses_v1_auto`                    | `/v1/responses`          | 200     | 78ms  | 同上                                                                                   | **无**                                      |
| `chat_v1_web_search_tool`              | `/v1/chat/completions`   | **422** | 69ms  | `invalid_request_error`                                                                | 无                                          |
| `anthropic_web_search_20250305`        | `/anthropic/v1/messages` | 200     | 90ms  | `server_tool_use`,`web_search_tool_result`,`web_search_result`；`stop_reason=end_turn` | **有**（https_citations=7；Anthropic 块）   |
| `responses_root_required`              | `/responses`             | 200     | 303ms | 工具回声 `web_search`                                                                  | **无**                                      |
| `responses_root_web_search_2025_08_26` | `/responses`             | 200     | 91ms  | 工具回声 `web_search_2025_08_26`                                                       | **无**                                      |
| `anthropic_type_web_search`            | `/anthropic/v1/messages` | **422** | 70ms  | `invalid_request_error`                                                                | 无                                          |

归类（与 2026-09-20 同形，本轮复验）：

- Responses 200 工具回声 = **协议／结果不足**，不是传输失败，也不得写成「模型不支持」。
- Chat Completions 422 = 该端点不接受内置 `web_search` 工具类型。
- Anthropic `web_search_20250305` = **可承载原生搜索**。

协议探针测试只断言「至少一行含 `status=`」，**不能**单独当作 G03 全项通过。

### 适配器 live

K12 隔离子请求走 Anthropic Messages（`POST https://api.deepseek.com/anthropic/v1/messages`），主对话仍是 Chat Completions。

| 项           | 观察                                                                                                                                                                                                                                    |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HTTPS        | `true`                                                                                                                                                                                                                                  |
| HTTP         | 200；`content_type=application/json`；`json_ok=true`；`raw_len=87855`                                                                                                                                                                   |
| 终止／状态   | `stop_reason=tool_use`；`error=absent`                                                                                                                                                                                                  |
| 用量         | prompt_tokens=28786；completion_tokens=270（探针在运行时外，未入 Iris `K06` 账本）                                                                                                                                                      |
| 内容块类型   | `message`, `server_tool_use`, `direct`, `web_search_tool_result`, `web_search_result`, **`web_search_tool_result_error`**                                                                                                               |
| 检索凭据     | **有**：`has_retrieval_credentials=true`；HTTPS 候选 **26** 条                                                                                                                                                                          |
| 引用主机去重 | `forecast.weather.com.cn`, `meteum.ai`, `pc.weathercn.com`, `tianqi.moji.com`, `weather.com`, `www.meteoblue.com`, `www.nmc.cn`, `www.theweathernetwork.com`, `www.weather.com.cn`, `www.yr.no`, `yandex.com`, `yandex.ee`, `yandex.uz` |

响应结构同时出现 `web_search_tool_result_error` 与可解析 HTTPS `web_search_result`。测试按现有判据通过；该错误块如实记下，未改适配器、未缩范围重跑。

## 四、窗口 §二 检查项（本轮实际覆盖）

| 检查项                        | 本轮                                                                                 |
| ----------------------------- | ------------------------------------------------------------------------------------ |
| 正常结束                      | Anthropic 探针 `stop_reason=end_turn`；适配器本次为 `tool_use`                       |
| 长度截断                      | 未运行                                                                               |
| 工具结束（客户端 `tool_use`） | 适配器见 `stop_reason=tool_use`；未做客户端续轮派发                                  |
| 缺失终止事件                  | 未运行                                                                               |
| 尾事件                        | 未运行（非流式；生产子请求仍 `stream:false`）                                        |
| 工具续轮字段                  | 未运行                                                                               |
| 稳定指令                      | 未运行                                                                               |
| **原生搜索事件／引用（G03）** | Anthropic `web_search_20250305` 与适配器 live **有**检索凭据。Responses 路径仍无检索 |
| 计费与权限（Q17）             | 用量可关联到本次探针；未扩大权限范围。未写入 Iris `K06` 账本                         |

## 五、适用范围结论

- **不是**「DeepSeek 端点机械与 live 均通过」。
- **不是** `G02`／`G03`／`Q17`／`D04` 关闭。
- 不得把 DeepSeek Responses 200 或 MiniMax 的 `{type:web_search}` 当成 DeepSeek 已搜。
- `C10` `Available` 不是 V05。
- 流式、截断、混合续轮、K06 入账、`streaming.rs` 搜索事件仍缺。
