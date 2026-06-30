# 2026-06-20 AI Agent Current Readiness

本文是阶段 1-10 完成后的当前态审计附录。`2026-06-20-ai-agent-issue-matrix.md` 保留为阶段 0 历史基线；本附录记录哪些 AAR 已被修复、哪些仍属于最小实现、哪些需要真实 LLM 或跨平台 E2E 继续验证，避免把历史基线误读为当前完成态。

## 当前口径

- 阶段 0 issue matrix 是历史基线，不代表当前缺陷仍全部存在。
- 当前 runtime 已贯通 `ConversationMemory`、`DeliberationState`、`WritingState`、`ResearchState`、`EvidencePipeline`、`SubAgentCoordinator`、`SkillTrustPolicy`、`SandboxProfile` 和统一工具权限审计主链路。
- 用户负担保持低：复杂任务状态、验证缺口、写作状态和研究状态都通过现有面板自然呈现；不新增强制流程或额外弹窗。
- `.md` 仍是最权威笔记源；AI 写入继续走确认、patch preview、content hash 和 audit。

## AAR 当前状态

| AAR     | 当前状态                 | 说明                                                                                                               |
| ------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| AAR-001 | 已修复                   | 工具执行统一经过 `ToolExecutionPipeline` 和 `PermissionDecisionEngine`。                                           |
| AAR-002 | 已修复                   | 多确认仍串行呈现，但确认事件携带真实 pending index/count。                                                         |
| AAR-003 | 已保持并强化             | subagent 并发读基线保留，并由 coordinator 增加资源锁与报告约束。                                                   |
| AAR-004 | 已锁定                   | depth deny 继续以 tool-role 错误返回，并有 contract 覆盖。                                                         |
| AAR-005 | 已修复                   | subagent 输入证据、报告、失败隔离和资源冲突进入 coordinator 主链路。                                               |
| AAR-006 | 已修复                   | Conversation Memory 持久化目标、偏好、决策、open threads 和 seq 追踪。                                             |
| AAR-007 | 已修复                   | session message content hash 成为可追踪字段。                                                                      |
| AAR-008 | 已修复                   | retract 边界拒绝 from_seq <= 0 语义。                                                                              |
| AAR-009 | 已持久化，非门控         | DeliberationState 和 verification summary 已持久化并透出到 UI；当前不阻断 finalize，也不自动追加回答 caveat。      |
| AAR-010 | 已修复，需真实供应商 E2E | 协议 fixture 覆盖 Anthropic tool use/streaming；真实账号 E2E 仍需环境验证。                                        |
| AAR-011 | 已修复                   | Git skill install 禁 hooks/smudge，trust profile 进入能力闭环。                                                    |
| AAR-012 | 已修复                   | skill allowed-tools 只能缩小工具面，高风险 capability 走权限决策。                                                 |
| AAR-013 | 已修复                   | 检索诊断区分索引未准备好与真实错误，并进入 EvidencePipeline。                                                      |
| AAR-014 | 已修复                   | rendered fetch 命名和用户可见能力说明不再暗示 JS 渲染。                                                            |
| AAR-015 | 已修复为诚实分级         | SandboxProfile 区分 L0/L1/L2/unsupported，不宣称 OS 级沙箱已实现。                                                 |
| AAR-016 | 已修复为诚实分级         | cert pinning 不再被描述为默认已实现能力。                                                                          |
| AAR-017 | 已收敛                   | 前端任务 hook 通过端口分组和状态面板收敛，仍保留现有主入口。                                                       |
| AAR-018 | 已修复                   | running/paused 任务状态持续刷新，terminal 状态停止轮询。                                                           |
| AAR-019 | 已修复                   | run plan 状态真实进入 UI，不再只是内部记录。                                                                       |
| AAR-020 | 已锁定                   | 前端 contract 禁止 raw checkpoint、用户笔记全文和敏感字段进入状态面板。                                            |
| AAR-021 | 已修复                   | 误导性 execute stub 已删除或废弃，并由源码契约锁定。                                                               |
| AAR-022 | 部分修复                 | approve 与 reject 进入 tool audit，reject 使用真实 tool name；deny/timeout 生产分支仍需后续接入 permission audit。 |
| AAR-023 | 已修复并保持确认边界     | 写作状态展示范围、理由、风险、回滚方式；无确认不写 `.md`。                                                         |
| AAR-024 | 已保持                   | 非目标边界未扩张：不引入企业权限、云端 agent、DAG 平台或强制容器化。                                               |

## 当前仍需验证的边界

- 真实 LLM 供应商 E2E：Anthropic/OpenAI streaming、tool call 边界和长上下文退化需要带真实凭据的端到端环境继续覆盖。
- 跨平台子进程边界：L1 sandbox 的 timeout、env 清理和参数限制已有 contract，但 macOS、Windows、Linux 行为仍需发布前 smoke test。
- 长期性能：50+ 轮 memory 与复杂研究状态已有契约，真实大 vault 下的索引延迟和 UI 刷新成本仍需压测。
- 研究质量：EvidencePipeline 能标注可信度、新鲜度、冲突和结论边界，但结论质量仍受模型与来源质量影响。

## 用户无感衔接

- 多确认仍按一个一个确认处理，只把进度改为真实值。
- verification 未通过时不弹窗、不硬阻断普通回答；当前通过任务事件与 verification summary 呈现，预算或轮次耗尽仍保持可恢复暂停。
- 复杂任务、写作和研究状态只在有内容时出现，默认保持紧凑。
- 前端新增字段均为可选字段，旧调用方不读取也不受影响。
