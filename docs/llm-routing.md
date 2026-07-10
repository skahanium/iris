# LLM 路由与连通性

## 配置边界

路由配置保存在 SQLite `settings` 表的 `llm_routing` 键中，由 Rust `llm_config_*` 命令读写和迁移。前端不得通过通用 `settings_set` 写入该键。当前 schema 会兼容并迁移历史 scene 配置到能力槽位配置。

配置分为三层：

1. **Provider**：启用状态、显示名称、允许的自定义 HTTPS base URL、模型目录与能力覆盖。
2. **能力槽位**：例如 fast、writer、reasoner、long_context、vision、agent_tools；每个槽位选择 provider、模型与 reasoning 模式，并可配置 failover。
3. **任务解析**：AI Runtime 按 intent、上下文预算、图像、工具需求和隐私偏好解析最终模型路由。

API Key 不属于路由 JSON；它以 `iris.llm.{provider_id}` 服务名进入 Iris 本地 AES-256-GCM 凭据存储。

## 模型、推理与预算

模型目录、provider 刷新结果和用户确认能力共同决定模型是否可用于文本、视觉、长上下文或 reasoning。未知模型不会因名称猜测获得高风险能力。原始 chain-of-thought、`reasoning_content` 及 `<think>` 类块不作为普通对话内容持久化或展示。

解析后的路由包含输入/输出 token 预算与 `hybrid` 或 `long_context` 策略。`long_context` 优先当前笔记全文，检索证据作为补充；实际内容仍受预算控制。

## HTTPS 与连通性

- 自定义 provider 必须使用 HTTPS；`http://`、loopback HTTP 和通用 settings 写入会被拒绝。
- provider 连通性检测与模型验证是独立操作：前者检查端点与凭据，后者按指定模型发起受控文本或视觉探测。
- `connectivity_status` 返回脱敏的 LLM 状态、模型、场景和联网 provider 配置状态；不返回 API Key、笔记正文或完整 prompt。

## 联网证据

助手只通过 `web_search` 语义入口请求外网证据。`WebEvidenceBroker` 仅使用被显式映射为 `web.search` / `web.fetch` 的 provider；搜索、显式 URL 深读和抓取均进入该 broker。普通证据详情只展示引用、标题、安全 URL/域名、摘录和冲突说明；provider 内部标识、原始结果哈希与提取方式只在诊断路径出现。

## 相关 IPC

- `llm_config_get`、`llm_config_set`、`llm_config_test`、`llm_config_test_provider`
- `llm_model_registry_refresh`、`llm_model_validate`、`llm_model_confirm_capability`
- `connectivity_status`
- `web_evidence_provider_*`

命令参数与返回类型以 `src/types/ipc.ts` 和 `src/lib/ipc.ts` 为准。
