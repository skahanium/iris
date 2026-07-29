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

模型目录、provider 刷新结果和模型验证事实共同决定模型是否可用于文本、视觉、长上下文或 reasoning。未知模型不会因名称猜测获得高风险能力。原始 chain-of-thought、`reasoning_content` 及 `<think>` 类块不作为普通对话内容持久化或展示。只有 provider 显式给出的 reasoning summary 才可作为独立、受限长度且已脱敏的 Run 过程事件显示和恢复；它不参与下一轮模型输入，不能替代或反推原始推理。

解析后的候选保留输入/输出 token 预算。视觉直答和工具循环都从同一模型池筛选，并将图片消息原样交给选中的视觉模型。

## HTTPS 与连通性

- 自定义 provider 必须使用 HTTPS；`http://`、loopback HTTP 和通用 settings 写入会被拒绝。
- provider 连通性检测与模型验证是独立操作：前者检查端点与凭据，后者按指定模型发起受控文本或视觉探测。
- `connectivity_status` 返回脱敏的 LLM 状态、已选模型和联网 provider 配置状态；不返回 API Key、笔记正文或完整 prompt。

## 联网证据

联网开关只授予联网能力。当前 Run Intake 以排除优先的确定性规则解析 `offline`、`web_required`：本机运行时事实、对话元问题、用户已提供材料的转换与创作任务可以离线；其余外部事实一律进入 `web_required`。`web_preferred` 仅为历史 Run 的兼容读值，新 Run 不会由默认分类产生它。

`web_required` 在模型前做一次受预算约束的预取；证据充分时后续只有一次无工具模型生成，模型不能自行决定是否执行首轮搜索。仅在高风险来源门槛未满足时允许一次确定性的补充搜索。搜索与一次瞬态重试共享 20 秒 Run 预算，重试等待 250ms；鉴权、策略拒绝、Schema 错误和证据包超界不重试。页面正文抓取失败不会抹掉已取得的搜索摘要。

非严格的可选 Web 工具失败会返回结构化工具结果并产生非终态 `capability_degraded` 事件。`web_required` 无可核验证据时写入 `web_verification_failed` 后安全终态，不生成事实结论或伪引用。诊断仅记录联网模式、原因码、尝试次数、结果和耗时区间，不记录查询、笔记、原始 MCP 输出、端点或凭据。偶发降级与 MCP/harness/LLM 分流步骤见 [ops/web-capability-degradation.md](./ops/web-capability-degradation.md)；可执行 `npm run diagnose:web-degradation` 读取本地 `agent_run_events`。

助手只通过 `web_search` 语义入口请求外网证据。严格事实回答使用 Run-local `[W1]…[Wn]` 标注，界面会渲染为上标徽章，并在消息底部「来源」列出对应 HTTPS 标题（见 [design-system.md](./design-system.md) Web 引用契约）。`WebEvidenceBroker` 仅使用被显式映射为 `web.search` / `web.fetch` 的 provider；搜索、显式 URL 深读和抓取均进入该 broker。非严格工具循环先检查模型池中是否有支持工具调用的模型，再检查联网证据 provider；严格路径先验证证据 provider，再选择无工具回答模型。普通证据详情只展示引用、标题、安全 URL/域名、摘录和冲突说明；provider 内部标识、原始结果哈希与提取方式只在诊断路径出现。

MCP 的 Web 路径仍只承载显式 `web.search` / `web.fetch` mapping，并只由联网开关授权。通用 MCP 只读工具走独立的 `external.read` 路径：管理中心把服务端 `readOnlyHint` 视为候选声明而非行为证明，只允许名称与递归输入 Schema 通过副作用审查、且用户对精确 provider/tool/schema 二次确认信任的工具进入白名单 binding；Composer 必须逐 Run 显式提交 binding ID/hash，Accept 原子冻结用户信任位、provider transport/config、Schema、映射和输出策略。运行中不重新 discovery，不透传服务端 description，也不允许 Skills、分类域、local-only 或模型自行扩大工具面。Iris 拒绝声明或 Schema 暴露写入、发送、删除、日历变更、进程和 secret 的工具，但无法独立验证已信任第三方服务端是否忠实实现声明。

## 严格事实核验（v1.2.16）

联网开关只授予 `web_search` 能力；开启后，除本机运行时事实、对话元问题、用户已提供材料的变换与纯创作外，所有外部事实请求都使用 `web_required`。每次完成都必须绑定本 Run 的 HTTPS Web 证据；会话历史、摘要和旧引用不能充当新一轮的核验结果。模型可见、Run 关联和最终引用都来自同一份最多 8 条的证据包：单条摘录最多 2,000 字符，Web 工具结果专用上限为 32,000 字符，超过预算必须重新打包或拒绝，绝不静默截断。引用使用 Run-local 的 `[W1]…[Wn]`，不复用会话级编号。联网未开启、来源不足或来源冲突时，Run 以可重试的安全终态结束，不给出事实结论。

时效新闻、赛果、职位和价格优先使用官方/主办方来源；没有官方来源时至少需要两个独立可信来源一致。来源不充分时宁可拒答。

## 相关 IPC

- `llm_config_get`、`llm_config_set`、`llm_config_test`、`llm_config_test_provider`
- `llm_model_registry_refresh`、`llm_model_validate`
- `connectivity_status`
- `web_evidence_provider_*`
- `mcp_read_only_tools_discover`
- `mcp_capability_bindings_list`、`mcp_capability_binding_upsert`、`mcp_capability_binding_delete`

命令参数与返回类型以 `src/types/ipc.ts` 和 `src/lib/ipc.ts` 为准。
