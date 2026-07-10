# AI Agent Issue Matrix

| ID      | 来源     | 维度         | 组件                 | 声明                                               | 证据                         | 严重度 | 状态       | 目标阶段 | 验收方式                      |
| ------- | -------- | ------------ | -------------------- | -------------------------------------------------- | ---------------------------- | ------ | ---------- | -------- | ----------------------------- |
| AAR-001 | DeepSeek | 长对话       | run.rs               | checkpoint 过大且可能携带 raw checkpoint           | 源码复核发现恢复状态需摘要化 | 高     | 成立       | 阶段 3   | checkpoint 只保存摘要字段     |
| AAR-002 | MIMO     | 工具         | tool_policy.rs       | ToolPolicy 与 permission preflight 需要同源        | 人工复核确认授权路径分散     | 高     | 部分成立   | 阶段 2   | 工具执行前统一预检            |
| AAR-003 | 人工复核 | subagent     | run.rs               | join_all 并发需要写入冲突约束                      | 源码复核确认只允许安全并发读 | 中     | 成立       | 阶段 7   | 并发写入被拒绝                |
| AAR-004 | 源码复核 | agent 权限   | agent_permissions.rs | 权限 profile 必须可审计                            | agent 权限表已形成最小实现   | 中     | 成立       | 阶段 5   | 权限审计仅记录脱敏摘要        |
| AAR-005 | DeepSeek | 工具         | harness_confirm.rs   | 确认队列需可恢复                                   | 多工具确认恢复路径已验证     | 中     | 部分成立   | 阶段 2   | pending confirmation 可序列化 |
| AAR-006 | MIMO     | 复杂推理     | model_gateway        | Anthropic 流式适配需隔离 provider 差异             | Anthropic 工具块解析不同     | 中     | 需实验验证 | 阶段 4   | provider adapter 合同测试     |
| AAR-007 | 人工复核 | skills       | skills               | Skills 不能成为安装或代码执行通道                  | prompt-only 边界写入产品规则 | 高     | 成立       | 阶段 8   | SKILL.md 范围是事实源         |
| AAR-008 | 源码复核 | 检索         | retrieval_broker     | 检索证据必须带来源 span/hash                       | 评测覆盖 source span 与 hash | 高     | 成立       | 阶段 6   | RAG fixture 合同测试          |
| AAR-009 | 人工复核 | 前端协作状态 | useAssistantTasks    | useAssistantTasks 与 useAgentTaskStatus 字段需兼容 | 前端新增字段均为可选字段     | 中     | 不成立     | 阶段 9   | TypeScript 类型检查           |
