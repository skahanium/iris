# AI Agent System Remediation

本文记录阶段 0 的审计范围和未来架构实体。范围覆盖长对话、复杂推理、subagent、工具、skills、检索、文件权限、agent 权限、沙箱、前端协作状态。

目标实体包括 ToolExecutionPipeline、PermissionDecisionEngine、ConversationMemory、DeliberationState、WritingState、ResearchState、EvidencePipeline、SubAgentCoordinator、SkillTrustPolicy、SandboxProfile。

## 阶段

- 阶段 1：清理虚假运行时和过期入口。
- 阶段 2：建立工具执行与权限预检边界。
- 阶段 3：补齐长对话 checkpoint 与恢复语义。
- 阶段 4：引入 DeliberationState 与验证状态。
- 阶段 5：收敛文件权限和写入确认。
- 阶段 6：拆分 WritingState、ResearchState 和 EvidencePipeline。
- 阶段 7：定义 subagent 协调和并发写入约束。
- 阶段 8：形成 SkillTrustPolicy 与 prompt-only Skills 边界。
- 阶段 9：补齐前端协作状态和可恢复任务 UI。
- 阶段 10：将沙箱能力、权限提示和审计记录收口为发布合同。
