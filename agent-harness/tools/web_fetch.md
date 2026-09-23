# web_fetch

<!-- iris:object T12 kind=rules file=true -->

`web_fetch` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T12 -->

<!-- iris:object T12 kind=tool name=web_fetch owner=M07 -->

## 用途与语义归属

两路 URL 共用批量正文读取与窗口续读。语义归属「工具」，责任模块 `M07`；目录身份由 [catalog.mjs](../catalog.mjs) 登记，语义归属与目标合同见 [docs/agent-architecture.md](../../docs/agent-architecture.md) §5、§6.2。

它是**正文读取**动作，与 `web_search`（发现）分工互补：搜索候选或用户明确提供的 URL 都由这里读取正文，因此「原生发现的页面」与「MCP 发现的页面」共用同一条读取路径，不因来源渠道产生第二条抓取通道（`N08`）。只有本工具取得正文的选中页面才登记为可引用证据；搜索片段不冒充完整正文。

## 参数与消费

| 参数        | 类型                          | 必填 | 消费事实                                                                                   |
| ----------- | ----------------------------- | ---- | ------------------------------------------------------------------------------------------ |
| `urls`      | string 数组（`minItems = 1`） | 是   | 本次 Run 候选或用户消息中明确给出的公开 HTTPS URL；空数组在派发前拒绝                      |
| `startChar` | integer（`minimum = 0`）      | 否   | 从正文第几个字符开始返回；`excerptWindow.nextStartChar` 可直接回填，用于续读同一页面后半段 |

`input_schema` 声明 `additionalProperties: false`。主链路只从 `urls` 取字符串项、非字符串项被丢弃，是否算「明确拒绝」属于目标合同对参数的要求（`G01` 的口径：暴露的参数必须被消费或明确拒绝）。

读取窗口的单位与边界由实现固定：单页快照上界 12 000 字符，单次窗口返回上界 2 000 字符，`excerptWindow` 回报 `startChar`、`endChar`、`returnedChars`、`nextStartChar`、`truncated`、`snapshotChars` 与上游完整性标记。

## 授权

- `C04` 是唯一授权决定者；运行期准入门槛是能力 `web.search`，联网开关关闭时原生抓取与 MCP 抓取都不可达。
- 目录声明的 `access_level = Network` 只是展示元数据，不得扩大 Run 授权。权限原子映射为 `WebSearch`，风险级别 `Low`；不需要确认（`requires_confirmation = false`）。
- URL 安全校验在派发前执行：仅接受 `https://`（先做规范化：去 fragment、去默认端口），非 HTTPS 一律拒绝。因此「模型临时编造 HTTP 地址」不会被静默抓取。
- 请求 URL 必须与返回条目 canonicalize 后一致（`K13`、`K18`）；只接纳被显式映射为 `web.fetch` 且通过诊断的 provider。

## 副作用

对外发起网络请求；向内保存本轮页面快照（`C22` 的来源身份与阅读范围）与审计事件。快照保存的是受控正文切片与内容哈希，不写 `.md`、不写长期记忆、不外发笔记内容。审计记录只保留结构化事实（成功页面、失败项、窗口位置），不把整页正文复制进审计或诊断（`K17`）。

## 预算

- 单一预算分类 `Network`，与 `web_search` **共用** `max_network_tool_calls` 类别额度（三类预设当前均为 6，`K06`、`Q19`）；一次批量读取记一次网络类派发，内部候选 provider 的回退不各记一次。
- 需要联网观察的 Run 会在启动前预留网络类额度给宿主的最小观察（搜索加抓取），因此留给模型自主读取的额度可能少于类别上限。
- 批量读取的边界按 URL 数在派发时计算：一次派发只抓取尚未持有快照的 URL，并受本轮页面槽位上限约束（当前每 Run 最多 12 页快照）；续读（`startChar > 0`）不重新抓取页面，只读取已有快照。
- 单次派发有界（当前 20 秒）；每个候选 provider 与整批另有更短的时间预算。数值属于受控配置，本卡片不新设数值。
- 备用派发入口的 `max_web_fetches` 与主链路不是同一上限；任何「最多抓四篇／五篇」的说法都不是网页主链路的全局上限（`Q19`）。

## 幂等

- 同一 URL 在一次 Run 内只建立一份快照：重复提交同一 URL 不重新抓取，直接从快照读取窗口，因此续读与重复调用不会反复消耗抓取额度。
- 该性质不跨 Run，也不在站点内容变化时保证返回相同正文。
- 快照建立与读取分离，使重复读取不产生新的外部请求；这不等于实现内部重试完全不会重复请求（见「失败反馈」）。

## 取消

派发边界在开始前检查 Run 取消标志；进行中的外部调用可请求取消，迟到结果保留为旧修订观察（`C03`）。批量读取中取消不返回半成品作为完整结果。

## 失败反馈

按可恢复错误反馈，一次失败不终止任务（`C14`、`N16`）：

| 情形                             | 反馈                                                                                                             |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 联网未授权                       | 明确拒绝，不静默改为本地读取                                                                                     |
| `urls` 缺失或为空                | 派发前拒绝（`tool_arguments_invalid`）                                                                           |
| 任一 URL 不是公开 HTTPS          | 整批在派发前被拒绝（`web_url_not_public_https`）；「只跳过非法项、保留合法项」**不是**当前行为，需按目标合同核对 |
| 部分页面读取失败                 | 保留已取得的正文，同时返回失败项，便于换来源继续；不因一页失败使整个任务失败                                     |
| 快照不可用（续读时尚未建立快照） | 回报 `snapshot_unavailable`，并提示从 `startChar = 0` 先取得快照                                                 |
| 超出正文范围                     | 回报 `eof` 或 `range_out_of_bounds`，不伪造续读成功                                                              |
| 传输成功但正文为空或不可解析     | 分别记录传输成功与结果可用性；返回阅读窗口与完整性，不把摘要包装成完整抓取                                       |
| 超时／网络类错误                 | 带退避与剩余预算约束重试；不重新抓取全部成功页面                                                                 |

## 暴露规则

通用能力目录项（`surface=general`），位于目录的 `web` 组；只在联网获准时进入工具面，并记录工具面版本（`C16`、`K10`）。它不是发现类动作，不占用每模型回合的发现上限。

在受限子任务中，本工具**当前不在**子 Run 的只读白名单内，子任务的网页正文核验因此受限（`Q07`），目标合同要求子任务具备获准的 `web_fetch`。

## 源码落点

- 目录定义：[tool_catalog/web.rs](../../src-tauri/src/ai_runtime/tool_catalog/web.rs)。
- 主链路实现：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `execute_web_tool`（读取 `urls`、`startChar`，查缓存快照并决定是否抓取）与 [run_tool_loop/web_reading.rs](../../src-tauri/src/ai_runtime/run_tool_loop/web_reading.rs)（`PageSnapshot`、`cached_fetch_urls`、`observe_web_pages`、`excerptWindow`）。
- 备用派发入口：[tool_dispatch/web.rs](../../src-tauri/src/ai_runtime/tool_dispatch/web.rs) 的 `web_fetch_tool`（只消费 `urls` 与 `ctx.max_web_fetches`，无窗口续读）。
- 抓取与回退：[web_evidence_broker.rs](../../src-tauri/src/ai_runtime/web_evidence_broker.rs)（MCP 提取优先，保留 native safe fetch 回退）。
- URL 校验：[run_tool_loop.rs](../../src-tauri/src/ai_runtime/run_tool_loop.rs) 的 `validate_public_fetch_urls`。

## 当前状态

- 文档成熟度 `draft`（见「维护规则」：[README.md](../README.md) §八）。
- 实现状态 `implementation.state = present`：目录项 `Dispatchable`，主链路实现批量读取与窗口续读。这只描述静态源码事实，不代表合同正确或用户任务可用。
- 与目标合同的差异与**待核对**项：
  1. 备用派发入口（`dispatch_tool_inner` 的 `web_fetch` 分支）不消费 `startChar`，也不建立运行窗口快照；生产主链路在派发前拦截该工具，因此该入口不是主路径，但两个入口的行为并不等价。
  2. 非 HTTPS 项导致整批拒绝，而不是保留合法 URL 的读取结果。
  3. 备用入口的 `max_web_fetches` 与主链路槽位上限不同源，口径需要统一说明（`Q19` 已明确「最多抓四篇／五篇」不是主链路上限）。
- 依赖未解：双路 URL 都能读取依赖两路发现的线索进入证据链，而原生搜索事件尚未接入（`G03`）；本工具自身不解决该缺口。
- 验证结果：本卡片不声明任何验证结论；本轮为基线 `2670739f` 的静态核对，未执行新实验、未运行付费实网评测。

## 相关合同

本工具消费 `K09`（联网授权与查询外发）、`K13`（网页读取窗口与完整性）。相关对象：`M07`（检索与证据）、`C21`（网页读取与提取）、`C20`（双路网页搜索协调）、`C18`（MCP 连接与传输）、`C22`（证据与出处管理）、`C17`（调用派发与观察）、`K06`（统一预算账本）、`K10`（工具面版本与派发观察）、`K18`（MCP 传输与外部工具边界）、`N08`、`Q07`、`Q19`。

<!-- iris:end T12 -->
