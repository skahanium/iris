# V05 MiniMax-M3 适配器 live（2026-09-23 1A 窄窗口）

本文件是 [2026-09-19 V05 逐端点实网验证窗口](./2026-09-19-v05-live-window.md) **1A 窄窗口**书面批准后的执行记录，不是关闭证明。

- 承接：`G03`／`Q17`／`K12`／`D04`
- 本轮只跑已有 `#[ignore]` 探针 `live_minimax_m3_adapter_returns_https_citations`
- **不关闭** `G02`、`G03`、`Q17`、`D04`；不把 `C10`／`C11`／`K12` 标为 `verification.passed`
- 不把 `npm run agent:eval:live` 当 V05；生产目录仍无「带工具的 Responses」主对话端点
- 不含 API Key、笔记正文、私有推理；引用只记去重主机名

## 一、本轮批准对照

| #   | 窗口要求     | 本轮事实                                                                                        |
| --- | ------------ | ----------------------------------------------------------------------------------------------- |
| 1   | 端点范围     | 仅 MiniMax-M3；DeepSeek-Flash 见同日 DeepSeek 记录（凭据缺失跳过）                              |
| 2   | 费用上限     | 本窗口不另设数字上限；本条实际 **1** 次适配器 live 请求                                         |
| 3   | 凭据         | 本地加密存储 `iris.llm.minimax` 密文存在且可解密；值未写入本文件、日志或仓库                    |
| 4   | 原生搜索厂商 | MiniMax-M3；主对话目录族仍是 `OpenAiCompatibleChatCompletions`（`https://api.minimaxi.com/v1`） |

## 二、命令与结果

```text
cargo test --manifest-path src-tauri/Cargo.toml --lib live_minimax_m3_adapter_returns_https_citations -- --ignored --nocapture
```

`ok. 1 passed; 0 failed`；墙钟约 **14.36s**。

K12 隔离子请求：`POST https://api.minimaxi.com/v1/responses`（适配器把 chat API base 接到 Responses 路径）。诊断提示：`What is the weather in Shanghai?`。

| 项           | 观察                                                                                                                                                                   |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 主机／路径   | `api.minimaxi.com/v1/responses`                                                                                                                                        |
| 协议         | OpenAI Responses（子请求）；主对话仍是 Chat Completions                                                                                                                |
| HTTPS        | `true`                                                                                                                                                                 |
| HTTP         | 200；`content_type=application/json; charset=utf-8`；`json_ok=true`；`raw_len=22865`                                                                                   |
| 终止／状态   | `status=completed`；`error=null`                                                                                                                                       |
| 用量         | prompt_tokens=7273；completion_tokens=551（探针在运行时外，未入 Iris `K06` 账本）                                                                                      |
| 内容块类型   | `message`, `output_text`, `web_search_call`, `search`, `url_citation`, `text`, `web_search`                                                                            |
| 检索凭据     | **有**：`has_retrieval_credentials=true`；HTTPS 候选 **10** 条                                                                                                         |
| 引用主机去重 | `m.sohu.com`, `sh.people.com.cn`, `www.163.com`, `www.nearweather.com`, `www.nmc.cn`, `www.shhuangpu.gov.cn`, `www.thepaper.cn`, `www.weather.com.cn`, `yandex.com.tr` |

请求结构见证（C26 脱敏）：出站含 Responses `tools`／`web_search` 子请求身份；响应 JSON 顶层键含 `output`、`usage`、`status`、`tools`（仅键名，无正文、无 Key）。

## 三、窗口 §二 检查项（本轮实际覆盖）

本轮是 **已登记适配器的一条 live 探针**，不是 MiniMax 全协议矩阵。未跑的项记「未运行」。

| 检查项                        | 本轮                                                                                            |
| ----------------------------- | ----------------------------------------------------------------------------------------------- |
| 正常结束                      | Responses `status=completed` 原样出现在结构摘要中                                               |
| 长度截断                      | 未运行                                                                                          |
| 工具结束（客户端 `tool_use`） | 未运行（服务端 `web_search_call`，单请求内完成）                                                |
| 缺失终止事件                  | 未运行                                                                                          |
| 尾事件                        | 未运行（非流式；生产子请求仍 `stream:false`）                                                   |
| 工具续轮字段                  | 未运行                                                                                          |
| 稳定指令                      | 未运行                                                                                          |
| **原生搜索事件／引用（G03）** | **有可核实检索凭据**：`web_search_call` + HTTPS `url_citation`。模型自行编造 URL 不能解释该结构 |
| 计费与权限（Q17）             | 用量可关联到本次探针；未扩大权限范围。未写入 Iris `K06` 账本                                    |

## 四、适用范围结论

- **不是**「本端点机械与 live 均通过」。
- **不是** MiniMax 全矩阵 V05，也**不是** `G02`／`G03`／`Q17`／`D04` 关闭。
- `C10` `Available` 不是 V05。
- 流式、截断、混合续轮、主对话 Responses、K06 入账、`streaming.rs` 搜索事件仍缺。
