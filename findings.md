# Agent Harness 重构发现

- 最终规格禁止生产路径保留旧执行 IPC、双写、隐式当前文档、Research executor 与场景/intent 路由。
- 当前新 Run 基础与旧 task/harness/research/session 体系仍同时存在；审计报告见 `docs/audits/2026-07-13-agent-harness-refactor-gap-analysis.md`。
- 已验证正常域新 Run 使用 `assistant:run_event`，正常 session 解析不再用 scene/note_path 过滤，新直接 Run 已改用 capability requirements 而非 `AgentIntent::Chat`。
- 涉密 CEF v1 把 document_path 写入线程正文和索引。现已升级为 v2：Conversation 只有稳定 UUID identity；Turn/Run/Event/Evidence 全部在 CEF；旧文件读取时解密、转换、临时 CEF 验证后原子替换。
- 当前前端仍在调用旧 classified thread IPC 和普通 `session_*`，必须在统一 Session API 切换窗口一并删除。
