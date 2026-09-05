# LLM 路由与连通性

## 配置边界

路由配置保存在 SQLite `settings` 表的 `llm_routing` 键中，由 Rust `llm_config_*` 命令读写和迁移。前端不得通过通用 `settings_set` 写入该键。当前 schema 为 v6：历史能力槽、槽位故障切换、scene、上下文策略和虚构评分策略会在读取时迁移为统一模型池；v5 及更早版本会将 `defaultModel` 提升为首项，再追加稳定排序的已启用模型，写回唯一的 `candidateOrder`。

配置由以下事实构成：

1. **Provider**：启用状态、显示名称、允许的自定义 HTTPS base URL、模型目录与能力覆盖。
2. **已启用模型池**：每个 provider 的 `enabledModels` 组成唯一候选池；`candidateOrder` 是持久化的主模型、备用 1、备用 2…顺序，不再写入 `defaultModel`。
3. **任务要求**：AI Runtime 从 Run Envelope 计算流式、工具、视觉、推理与上下文预算要求，过滤不满足条件的模型，并至多保留前三个合格候选。显式 `modelOverride` 固定到精确 provider/model，不参与自动故障切换。

LLM 仅在首个可见 token、工具调用或 provider continuation 之前切换。连接失败、超时、408、429、5xx、临时不可用和空/无效响应可推进到备用；401/403 跳过同一 provider 的其余模型后才尝试其他 provider。普通 4xx 业务错误、用户取消及已有可见输出不会切换。连续两次瞬态失败会打开 `llm:{provider}:{model}` 熔断器 30 秒；成功会关闭该模型熔断器，不改变 MCP provider 的熔断语义。每次切换只持久化安全的 capability、来源/目标 provider、目标 model、原因码与尝试序号。

API Key 不属于路由 JSON；它以 `iris.llm.{provider_id}` 服务名进入 Iris 本地 AES-256-GCM 凭据存储。

## 模型、推理与预算

模型目录、provider 刷新结果和模型验证事实共同决定模型是否可用于文本、视觉、长上下文或 reasoning。未知模型不会因名称猜测获得高风险能力。原始 chain-of-thought、`reasoning_content` 及 `<think>` 类块不作为普通对话内容持久化、展示、记忆、证据账本或归因引用。若 provider 协议明确要求同一 Run 的 continuation，`reasoning_content` 只可随该协议 continuation 传递；它绝不进入后续历史轮次。只有 provider 显式给出的 reasoning summary 才可作为独立、受限长度且已脱敏的 Run 过程事件显示和恢复，不能替代或反推原始推理。

内置 MiniMax 端点按公开模型协议显式分族：M3 只提供 `off/auto`，M2.x 的 thinking 不可关闭并固定为 `on`；两者都用顶层 `reasoning_split=true` 隔离推理详情，只有 M3 发送 `thinking.type`。流式 `reasoning_details` 按同一块的累积快照或增量片段合并，完整 assistant 续轮状态仅在同一 Run 的工具 continuation 中回放。模型名出现在自定义端点时不会继承这些内置能力。

解析后的候选保留输入/输出 token 预算。视觉直答和工具循环都从同一模型池筛选，并将图片消息原样交给选中的视觉模型。

## HTTPS 与连通性

- 自定义 provider 必须使用 HTTPS；`http://`、loopback HTTP 和通用 settings 写入会被拒绝。
- provider 连通性检测与模型验证是独立操作：前者检查端点与凭据，后者按指定模型发起受控文本或视觉探测。
- 自定义 provider 的上述探测不验证 tools、tool-call continuation 或 reasoning 协议；当前一律保持 chat-only，设置页在验证成功后仍持续显示该限制。
- `connectivity_status` 返回脱敏的 LLM 状态、已选模型和联网 provider 配置状态；不返回 API Key、笔记正文或完整 prompt。

## 联网证据

联网开关只授予 `web.search` 能力。当前 Run Intake 以排除优先的确定性规则解析 `offline`、`web_preferred`、`web_required`：本机运行时事实、对话元问题、用户已提供材料的转换与创作任务可以离线；一般外部事实可进入 `web_preferred`；明确联网、强时效或高风险当前事实进入 `web_required`。有 Web 工具面时使用同一个 `AgentToolLoop`，Host 不在模型前另做预取或领域规划。

模型工具面拆为两个单一职责动作：`web_search { query }` 只返回当前 Run 候选，`web_fetch { urls }` 只读取当前 Run 候选或用户明确提供的 HTTPS URL。两者共用联网授权、network 分类预算、`WebEvidenceBroker`、冻结 Provider 顺序和 evidence ledger；搜索片段不是证据，只有抓取到 URL 匹配的实质正文才登记为 `Wn`。一批 URL 部分失败时，工具观察保留成功正文、失败 URL、剩余证据要求和预算，模型可以换源继续。

Web 工具失败会返回可行动的结构化观察并可产生非终态 `capability_degraded` 事件。普通时效事实取得一份与核心结论相关的当前 Run 正文并精确引用即可回答；高风险当前事实、CitationCheck 或用户明确要求交叉核实才要求官方来源或两个独立域名。来源不足时丢弃未验证草稿，以无 citation map 和 source summary 的自然限制说明完成；Provider、持久化、权限或内部状态损坏才使用红色失败。诊断只记录联网模式、能力、原因码、尝试次数、结果和耗时区间，不记录查询、笔记、原始 MCP 输出、端点或凭据。

严格来源路径在 `bind_validated_content` 前密封正文，来源 repair 使用同一 ToolLoop 的一次修复槽；通过后只发布一次，失败不发送 `AnswerReset`。普通非严格回答继续实时流式。现代消息只按最终 `evidence_refs_json` 投影来源；显式空数组保持无来源，只有字段缺失的旧消息可以显示历史来源组。`WebEvidenceBroker` 仅使用被显式映射为 `web.search` / `web.fetch` 的 provider。普通来源区只显示最终实际引用的可点击 HTTPS 标题，不显示摘录、搜索词、工具参数、原始输出或内部推理。

MCP 的 Web 路径仍只承载显式 `web.search` / `web.fetch` mapping，并只由联网开关授权。通用 MCP 只读工具走独立的 `external.read` 路径：管理中心把服务端 `readOnlyHint` 视为候选声明而非行为证明，只允许名称与递归输入 Schema 通过副作用审查、且用户对精确 provider/tool/schema 二次确认信任的工具进入白名单 binding；Composer 必须逐 Run 显式提交 binding ID/hash，Accept 原子冻结用户信任位、provider transport/config、Schema、映射和输出策略。运行中不重新 discovery，不透传服务端 description，也不允许 Skills、分类域、local-only 或模型自行扩大工具面。Iris 拒绝声明或 Schema 暴露写入、发送、删除、日历变更、进程和 secret 的工具，但无法独立验证已信任第三方服务端是否忠实实现声明。

内置 Tavily 预设使用官方 HTTPS MCP `https://mcp.tavily.com/mcp/`，将加密凭据服务 `iris.mcp.tavily` 作为必填 `Authorization: Bearer` 头（密钥绝不进入 URL）。其 `tavily_search` 映射 `query` / `max_results`，`tavily_extract` 映射 `urls`、`extract_depth: basic` 与 `format: markdown`。选择预设只填写配置，既不会自动启用 provider，也不会授予联网权限。

## 当前事实证据分级（v1.3.0）

联网开关只授予 `web_search` 与 `web_fetch` 的共同能力边界；每次完成都必须绑定本 Run 的 HTTPS Web evidence，会话历史、摘要和旧引用不能充当新一轮核验结果。搜索候选每次最多 4 条、每 Run 最多 8 条且不占 evidence；整个 Run 最多注册 12 条正文 evidence，单条摘录最多 2,000 字符，Web 工具结果专用上限为 32,000 字符。Run-local `W1…Wn` 每轮重新编号，不复用会话级编号或数据库裸 ID。

普通近期电影、体育和新闻等 `VolatileExternalFact` 不因领域名称进入特殊路由，一份相关正文和精确引用即可完成。高风险事实、CitationCheck、显式要求官方来源或交叉核实的请求才提高到官方/双域名门槛。联网未开启、只有片段、来源冲突或严格门槛不足时不得伪造事实结论。

## 相关 IPC

- `llm_config_get`、`llm_config_set`、`llm_config_test`、`llm_config_test_provider`
- `llm_model_registry_refresh`、`llm_model_validate`
- `connectivity_status`
- `web_evidence_provider_*`
- `mcp_read_only_tools_discover`
- `mcp_capability_bindings_list`、`mcp_capability_binding_upsert`、`mcp_capability_binding_delete`

命令参数与返回类型以 `src/types/ipc.ts` 和 `src/lib/ipc.ts` 为准。
