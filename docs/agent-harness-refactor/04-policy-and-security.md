# 权限、文档策略与安全规格

## 1. 一个策略引擎

Document Policy、Tool Permission、Security Domain、Web Toggle、Skill Capability 和用户确认必须汇入同一个 `PolicyDecisionEngine`。任何 executor、MCP adapter 或 Tool Dispatcher 不得自行放宽决定。

策略输出至少包含：

```text
auto_allowed capabilities
confirmation_required effects
denied effects and stable reasons
effective document scopes
security domain restrictions
grant source and expiry
```

## 2. 文档能力矩阵

每个文档范围分别控制：

```text
discover
read
send_to_model
cite
propose_change
apply_change
```

继承顺序：Vault 默认 → 最近的文件夹/语料规则 → 文档覆盖。每个能力独立解析；最具体规则生效，同一层级显式 deny 优先。

### 默认值

普通文档默认允许全部六项能力。这里的 `apply_change = allow` 只表示该资源具备“可被 Agent 修改”的资格，仍不构成本次用户同意；每次实际写回继续受风险规则和变更计划确认约束。用户可在文件夹或文档层把 `apply_change` 设为 deny，届时即使确认弹窗也不能覆盖。

普通用户不需要逐文档设置。高级权限管理可以按文件夹覆盖，但不得成为首次使用 Agent 的前置步骤。

### 显式引用

`@` 只表达用户希望把资料纳入本轮，不提升资料本身的权限。被 `@` 的文档若禁止 read 或 send_to_model，Harness 必须说明原因并继续处理其余可用上下文。

## 3. 遵循等级与指令信任分离

文件夹资料角色保持：

```text
authority  = 规范依据
exemplar   = 范文样本
reference  = 参考资料
lookup     = 查阅资料
```

- 子文件夹可覆盖父文件夹。
- legacy `regulation` 映射为 authority，`general` 映射为 lookup。
- 未知值不得默认 authority，应拒绝配置或安全降级为 lookup 并报告诊断。

资料角色只决定“如何使用内容”，绝不决定“内容能否向 Agent 下命令”。所有文档、网页、工具结果和 MCP 结果必须以不可信 data/evidence 消息传入，禁止拼入 system 指令消息。

## 4. 工具风险与确认

### 自动允许

- 已授权范围内的本地只读检索和读取。
- 联网开关开启且 envelope 允许的 Web 搜索和抓取。
- 在内存中生成草案、建议和 patch preview。
- 不产生外部副作用的诊断。

### 变更计划确认

中风险写回以用户可理解的变更计划为确认单位，而不是以底层工具调用为单位。确认载荷必须冻结：

- 目标 Vault 和相对路径。
- 操作类型。
- 基准 content hash。
- 完整 diff 或等价结构化变更。
- 影响文件数量和可撤销方式。
- 计划哈希和到期时间。

确认后只能执行计划哈希完全一致的操作。目标、基准、参数、diff 或文件集合变化时，旧确认失效并重新展示。

### 始终逐次确认

- 删除、覆盖、批量移动或批量重命名。
- 覆盖 hash 已变化的文档。
- 外部消息、上传、发布或其他不可逆副作用。
- 跨安全域数据移动。

## 5. 会话授权

现有 `allow_for_session` 不能再用 `request_id` 模拟 Session。若保留高级授权，grant 必须包含：

```text
session_id
capability/effect
resource scope
risk ceiling
created_at / expires_at
revocation state
```

- 高风险效果禁止 Session grant。
- 默认 UI 不主动提供宽泛 Session grant。
- Session 删除、Vault 切换、权限规则变化或到期后自动失效。

## 6. 涉密域

- Session 与当前涉密文档同样硬解绑。
- 用户必须本轮明确 `@` 涉密文档，或从涉密编辑器动作显式传入目标。
- 保险库必须处于解锁状态，文档必须允许 read/send_to_model。
- 用户主动发起构成当前 Run 的送模同意，不跨 Run 继承。
- 使用当前选定 LLM Provider，不增加 Provider 白名单；请求必须 HTTPS。
- 禁止 Web、MCP、普通笔记、普通检索缓存、普通 Conversation Memory 和普通 Evidence Ledger。
- 涉密 Run、消息、checkpoint 和证据使用 CEF 加密持久化。
- 错误、事件、日志和 Provider 切换说明不得暴露涉密路径或正文。

## 7. 提示注入防护

关键词过滤只能作为辅助信号，不能作为安全边界。主要防线是：

- 系统策略与证据内容使用不同消息层级和明确数据封装。
- 证据永远不能改变 capability 集合、工具参数 Schema 或 Policy Decision。
- 工具参数在模型输出后重新做类型、路径、权限和效果验证。
- 规范依据中的“执行、上传、忽略限制”等文本仍只是被分析的内容。
- Prompt Guard 告警不得把正常法规文本误判为用户越权；即使检测漏过，策略边界仍生效。

## 8. 审计与脱敏

审计只记录：

- Run、Tool、Capability 和权限名称。
- 脱敏资源范围摘要。
- 风险等级、决定、grant 来源、结果和耗时。
- Provider/MCP 标识和错误分类。

禁止记录 API Key、Token、笔记正文、完整 prompt、加密密码、涉密路径和工具原始敏感载荷。

## 9. 策略测试不变量

- deny 无法被 `@`、Skill、MCP 或 Provider failover 覆盖。
- editor active note 从不参与权限范围推导。
- write preview 与实际 dispatch 参数不一致时执行为零。
- 网络关闭时所有 Native/MCP Web dispatch 为零。
- 涉密域所有普通数据库证据写入为零。
- 恶意证据内容不能新增工具、扩大范围或改变确认结果。
