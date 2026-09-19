# V05 MiniMax-M3 原生搜索实网探针（2026-09-20）

本文件是 [2026-09-19 V05 逐端点实网验证窗口](./2026-09-19-v05-live-window.md) 授权后的**执行记录**，不是关闭证明。

- 承接：`G03`／`Q17`／`K12`／`D04`
- 本轮只跑 **MiniMax-M3** 原生搜索协议探针（诊断调用，非全矩阵）
- **DeepSeek 未运行**（缺本轮 Key／费用／live 批准）
- **不关闭** `G02`、`G03`、`Q05`、`Q17`、`D04`；不把 `C20`／`C11`／`K12` 标为 verification passed
- 生产适配器计数仍为 0；搜索事件仍未进入 `streaming.rs`
- 不含 API Key、笔记正文、私有推理；检索正文只记长度与主机名

## 一、本轮批准对照

| # | 窗口要求 | 本轮事实 |
| - | -------- | -------- |
| 1 | 端点范围 | MiniMax-M3 原生搜索协议探针；DeepSeek 不在本轮 |
| 2 | 费用上限 | 用户声明本轮 MiniMax 无上限；实际只发了 **2** 次诊断请求 |
| 3 | 凭据 | 本地加密存储 `iris.llm.minimax` 密文存在且可解密；值未写入本文件、日志或仓库 |
| 4 | 原生搜索厂商 | MiniMax-M3；主对话目录族仍是 `OpenAiCompatibleChatCompletions`（`https://api.minimaxi.com/v1`） |

## 二、调用摘要

诊断提示（厂商文档示例，非用户笔记）：`What is the weather in Shanghai?`；`model=MiniMax-M3`；`stream=false`；超时 180s。

| 项 | 调用 1 | 调用 2 |
| -- | ------ | ------ |
| 名称 | `anthropic_minimaxi_com` | `responses_minimaxi_com` |
| 主机／路径 | `api.minimaxi.com/anthropic/v1/messages` | `api.minimaxi.com/v1/responses` |
| 协议 | Anthropic Messages | OpenAI Responses |
| 工具声明 | `tools: [{type:web_search_20250305, name:web_search}]` | `tools: [{type:web_search}]` |
| 请求结构 | 角色 `user`×1；`max_tokens=2048`；未流式 | `input` 字段存在；未流式 |
| HTTP | 200 | 200 |
| 耗时 | 1909 ms | 4324 ms |
| 关联 id | `06fde7b6018967af796cc0a8c8757978` | `06fde7b817051d34180063d32f4cf78a` |
| 终止／状态 | `stop_reason=end_turn` | `status=completed`（无 `stop_reason`） |
| 用量 | in 558 / out 139 / cache_read 128 | in 6276 / out 415 / total 6691 / cached 1292 |
| 内容块类型 | `message`, `text` | `web_search_call`, `search`, `message`, `output_text`, `url_citation`×10, `web_search` |
| 检索凭据 | **无** | **有**：`web_search_call`×1（`action.query=Shanghai weather today`，`status=completed`）；message 内 `url_citation`×10 |
| 引用主机（去重） | （无） | `www.msn.com`, `m.gmw.cn`, `th.thetimenow.com`, `www.toutiao.com`, `www.thepaper.cn`, `www.shhuangpu.gov.cn`, `www.shobserver.com`, `www.weather.com.cn`, `www.163.com` |

未调用：`api.minimax.io`（国内生产主机已给出可判定结果，未做第三跳）。

## 三、窗口 §二 检查项（本轮实际覆盖）

本轮是 **G03 原生搜索协议探针**，不是 MiniMax 全协议矩阵。未跑的项记「未运行」，不得用调用 2 的 200 填绿。

| 检查项 | Anthropic Messages（调用 1） | Responses（调用 2） |
| ------ | ---------------------------- | ------------------- |
| 正常结束 | `stop_reason=end_turn` 原样保留 | `status=completed`；无 Messages 式 `stop_reason` |
| 长度截断 | 未运行 | 未运行 |
| 工具结束（客户端 `tool_use`） | 未运行（服务端工具，无客户端续轮） | 未运行（单请求内完成搜索） |
| 缺失终止事件 | 未运行 | 未运行 |
| 尾事件 | 未运行（非流式） | 未运行（非流式） |
| 工具续轮字段 | 未运行 | 未运行 |
| 稳定指令 | 未运行 | 响应含 `instructions=null`；未做续轮 |
| **原生搜索事件／引用（G03）** | **协议／结果不足**：200 且只有 `text`，无 `server_tool_use`／`web_search_tool_result`／URL。模型正文表示没有实时天气——这是生成文字，**不是**「已搜」，也**不得**写成「模型不支持」 | **有可核实检索凭据**：`web_search_call` + HTTPS `url_citation`。模型自行编造 URL 不能解释该结构 |
| 计费与权限（Q17） | 用量可关联到本次调用 id；未扩大权限范围 | 同左；input_tokens 明显高于调用 1，与服务端检索一致。未把用量记入 Iris `K06` 账本（探针在运行时外） |

## 四、适用范围结论

- **不是**「本端点机械与 live 均通过」。
- **不是**「MiniMax 原生搜索已接入 Iris」。
- Anthropic Messages + `web_search_20250305` 在 **Iris 生产主机** `api.minimaxi.com` 上：请求被接受（200／`base_resp.status_code=0`），但**静默未检索**。归类为 **协议／结果不足**，不是传输失败，不是能力降级文案。
- OpenAI Responses `POST /v1/responses` + `{type:web_search}` 在同一主机上：**协议可承载原生搜索**，且返回了检索凭据。这只证明该诊断请求；不证明流式、续轮、账本或双路生产入口。
- 目录里 `MiniMax-M3` 仍是 Chat Completions 族。原生搜索若接入，应是 **K12 隔离子请求** 走 Responses，而不是把主对话改写成 Responses，也不是把品牌／`supports_tools` 推断为 `Available`。
- 机械层：`OpenAiShaped` 解析器已能识别 `web_search_call` + `url_citation`；生产 `production_native_search_adapter_count() == 0`，C10 对 MiniMax-M3 仍为 `adapter_absent`。

## 五、下一步（未做）

1. MiniMax-M3 生产适配器：对获准查询发 Responses 子请求，把 live 形状映射到 `SearchHit`，**不得**把 Anthropic 200 当成功。
2. 将服务端搜索事件纳入网关流式路径（`G03` 仍缺）。
3. DeepSeek 在 Key／费用／live 批准后再探针；不得用本文件代替。
4. 流式、截断、混合续轮、K06 入账仍缺，本文件不补跑。
