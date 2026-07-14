# LLM 路由与连通性

## 配置边界

路由配置保存在 SQLite `settings` 表的 `llm_routing` 键中，由 Rust `llm_config_*` 命令读写和迁移。前端不得通过通用 `settings_set` 写入该键。当前 schema 为 v5：历史能力槽、槽位故障切换、scene、上下文策略和虚构评分策略会在读取时迁移为统一模型池。

配置由以下事实构成：

1. **Provider**：启用状态、显示名称、允许的自定义 HTTPS base URL、模型目录与能力覆盖。
2. **已启用模型池**：每个 provider 的 `enabledModels` 组成唯一候选池；`defaultModel` 只是满足硬条件时的第一选择，不是能力槽绑定。
3. **任务要求**：AI Runtime 从 Run Envelope 计算流式、工具、视觉、推理与上下文预算要求，过滤不满足条件的模型；默认模型不合格时以稳定顺序选择其余候选，且仅在可重试传输失败后切换。

API Key 不属于路由 JSON；它以 `iris.llm.{provider_id}` 服务名进入 Iris 本地 AES-256-GCM 凭据存储。

## 模型、推理与预算

模型目录、provider 刷新结果和模型验证事实共同决定模型是否可用于文本、视觉、长上下文或 reasoning。未知模型不会因名称猜测获得高风险能力。原始 chain-of-thought、`reasoning_content` 及 `<think>` 类块不作为普通对话内容持久化或展示。

解析后的候选保留输入/输出 token 预算。视觉直答和工具循环都从同一模型池筛选，并将图片消息原样交给选中的视觉模型。

## HTTPS 与连通性

- 自定义 provider 必须使用 HTTPS；`http://`、loopback HTTP 和通用 settings 写入会被拒绝。
- provider 连通性检测与模型验证是独立操作：前者检查端点与凭据，后者按指定模型发起受控文本或视觉探测。
- `connectivity_status` 返回脱敏的 LLM 状态、已选模型和联网 provider 配置状态；不返回 API Key、笔记正文或完整 prompt。

## 联网证据

助手只通过 `web_search` 语义入口请求外网证据。`WebEvidenceBroker` 仅使用被显式映射为 `web.search` / `web.fetch` 的 provider；搜索、显式 URL 深读和抓取均进入该 broker。工具循环先检查模型池中是否有支持工具调用的模型，再检查联网证据 provider：前者失败返回“没有已启用模型满足当前任务所需能力”，后者返回“未配置可用的联网证据提供方”，不会误报模型服务故障。普通证据详情只展示引用、标题、安全 URL/域名、摘录和冲突说明；provider 内部标识、原始结果哈希与提取方式只在诊断路径出现。

## 相关 IPC

- `llm_config_get`、`llm_config_set`、`llm_config_test`、`llm_config_test_provider`
- `llm_model_registry_refresh`、`llm_model_validate`
- `connectivity_status`
- `web_evidence_provider_*`

命令参数与返回类型以 `src/types/ipc.ts` 和 `src/lib/ipc.ts` 为准。
