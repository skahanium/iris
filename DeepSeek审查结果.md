# Iris AI Agent 系统全面审查报告

> 审查日期：2026-06-20
> 审查范围：AI Agent 体系全链路（长对话、深度思考、SubAgent 调度、工具调度、Skills 系统、网络检索、文件权限、安全权限、沙箱、整体协作）

---

## 一、总体评价

Iris 的 AI Agent 系统架构设计**扎实且层次分明**，体现了对安全性和工程质量的认真投入。模块划分清晰（`ai_types` → `ai_runtime` → `ai_harness` → `commands`），无循环依赖。安全防护层层递进（提示注入防御 → 工具策略 → 权限模型 → 网络隔离），但实现完成度不均——核心流程成熟，而部分子系统（权限模型强制执行、沙箱、子Agent 协调）仍存在明显缺口。

**整体评分：B+（良好，有改进空间）**

| 维度 | 评分 |
|------|------|
| 长对话能力 | C+ |
| 长难任务深度思考解决能力 | B+ |
| SubAgent 运行调度能力 | B |
| 工具调度能力 | B+ |
| Skills 安装及调度使用能力 | A- |
| 网络检索能力 | A- |
| 文件读写权限设置及读写能力 | A- |
| Agent 权限设置安全性 | C |
| 沙箱设置及运行情况 | D+ |
| Agent 内部整体运行协作能力 | B |

---

## 二、各维度详细评价

### 2.1 长对话能力 — 评分：C+

**做得好的：**
- 会话通过 SQLite 持久化，按 `scene:note_path` 键值隔离，支持跨应用重启恢复
- 支持 `retract_messages`（撤回消息）、`rename_session`（重命名会话）
- 会话有过期清理机制（`purge_expired`）

**存在的严重问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **消息数量无上限，数据库无限增长** | HIGH | `session.rs:105-137` — `append_message` 无任何截断或数量约束 |
| **无 Token 感知的消息窗口管理** | HIGH | 会话层不计算 Token，不做自动摘要；仅 Harness 层对超过10条消息做粗略压缩（`harness_support.rs:18-38`） |
| **`content_hash` 字段从未被填充** | BUG | `session.rs:105-137` — 字段定义存在但 `append_message` 从不设置，永远是 `None` |
| **无消息分页游标** | PERF | `recent_messages` 用 `ORDER BY seq DESC LIMIT N`，万条消息时需全表反向扫描 |
| **`retract_messages` 无参数下限校验** | BUG | `session.rs:325-340` — `from_seq=0` 会删除全部消息 |
| **`purge_expired` 无软删除模式** | INFO | 过期会话直接物理删除，无回收站机制 |

---

### 2.2 长难任务深度思考解决能力 — 评分：B+

**做得好的：**
- Harness 多轮循环（`run.rs:249-699`）支持复杂工具链调用，含 Token 预算和轮次上限
- DeepSeek Reasoning Content 被完整保留并在工具调用轮次中回传
- 反思（Reflection）机制：深度0可获一轮反思 + 一次奖励回合（`reflection.rs:30-141`）
- 中断后可保存检查点并恢复：`PausedBudget` / `PausedRecoverable` 状态
- 检查点恢复前有完整预检（vault scope hash、note paths、skills、permissions、model slot）

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **Token 估算降级方案过于粗略** | MEDIUM | `token_estimator.rs:23-33` — ASCII 按 chars/4 估算，CJK 按 chars*0.8，未考虑不同模型 tokenizer 差异 |
| **实际上下文窗口可能超出 LLM 限制后才被 Token 预算检测到** | MEDIUM | `run.rs` — Token 预算是累加器而非滑动窗口检查，消息数组可无限增长 |
| **工具执行期间的中途取消（abort）只在每轮开始时检查** | MEDIUM | `run.rs:793-799` — 长时间运行的工具调用期间无法被中止 |
| **规则意图检测脆弱，纯关键词匹配无 NLP 语义理解** | LOW | `context_planner.rs:180-203` |
| **Harness 检查点绕过脱敏验证** | SEC | `harness_support.rs:156-164` — `HarnessCheckpoint` 直接序列化含原始消息，未经 `validate_checkpoint` |

---

### 2.3 SubAgent 运行调度能力 — 评分：B

**做得好的：**
- `spawn_subagent` 作为 LLM 可用工具，自动从工具调用中分区处理
- 支持并行执行（`join_all`，`run.rs:451`），所有子Agent 在同一轮次并发运行
- 深度限制：depth >= 2 时工具策略层隐藏 `spawn_subagent`（`tool_policy.rs:124`）
- 子Agent 继承父Agent 的技能激活计划和任务策略（自主级别、工具上限等）

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **深度 >= 2 时子Agent 调用被静默丢弃，LLM 无错误反馈** | HIGH | `run.rs:426` — `if` 块被简单跳过，不推送 tool-role 错误消息 |
| **子Agent 不继承父Agent 已积累的证据** | MEDIUM | `run.rs:1016` — 只传 `cold_start_packets`，父Agent 调用 `search_hybrid` 等工具的结果完全丢失 |
| **父Agent 中止不会级联到并行运行中的子Agent** | MEDIUM | `run.rs:1008` — 子Agent 使用独立的 `{parent_id}-sub-{uuid}` request_id |
| **多个并行子Agent 可能同时修改同一笔记，无协调/锁机制** | MEDIUM | `run.rs:420-486` |
| **子Agent 预算分配简单（固定 60%），不按任务复杂度自适应** | LOW | `run.rs:1003-1006` |
| **子Agent 历史消息极简（仅一条 user 消息），丢失父对话上下文** | LOW | `run.rs:1020` |

---

### 2.4 工具调度能力 — 评分：B+

**做得好的：**
- 87个工具通过集中式 `TOOL_CATALOG` 管理，含 JSON Schema、访问级别、确认要求、场景亲和度
- 工具策略引擎（`tool_policy.rs`）多层次检查：能力匹配 → 自主级别 → 网络开关 → 技能白名单 → 深度限制
- 工具执行审计追踪完整（`tool_audit` 表），敏感数据（密钥、密码、token）用长度/hash 替代
- LLM API 调用含指数退避重试（3次，1s/2s/4s）+ 熔断器保护（5次连续失败后断路 30s）
- 工具调用解析含文本回退（`parse_tool_calls_from_content`）和 3 次连续解析失败保护

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **多确认工具只处理第一个，其余被静默跳过** | HIGH | `tool_turn.rs:9-29` + `run.rs:509-510` — LLM 一次调用 `insert_text_at_cursor` + `replace_selection`，仅第一个弹出确认框，第二个被 `continue` |
| **非 Web/Search 工具失败零重试** | MEDIUM | `tool_dispatch_impl.rs:58-79` — 仅 `web_search`、`fetch_web_page` 有重试；`read_note`、`list_vault` 等工具一次失败即永久失败 |
| **`conclude_reasoning` 出现时直接跳出循环，丢弃同轮其他工具调用** | LOW | `run.rs:371-376` |
| **非子Agent 工具严格串行执行，无并行优化** | INFO | `run.rs:507` — 刻意设计但可能影响性能 |
| **未知工具名仅返回通用错误，无日志或告警** | INFO | `tool_dispatch_impl.rs:111-175` — 可能错过提示注入攻击信号 |

---

### 2.5 Skills 安装及调度使用能力 — 评分：A-

**做得好的：**
- 支持四种安装源：URL（含 SHA-256 校验）/ Git（shallow clone）/ 本地（限制在家目录和 Vault 内）/ Registry（SkillHub）
- 安装含许可证合规检查（AGPL-3.0/GPL-3.0/MIT 等白名单）、关键能力拦截（脚本执行、依赖安装等自动禁用）
- 激活排名采用 BM25 关键词 + 向量相似度双阶段评分（`activation.rs:155-339`）
- 技能工作区隔离完善（`.iris/skills-workspaces/`），含符号链接攻击防护（`workspace.rs:326-365`）
- 技能内容注入有 12,000 字符截断限制（`prompt.rs:7`）
- 支持 Legacy Skill 迁移和兼容

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **Git clone 未禁用 hooks/smudge filters** | MEDIUM | `skills_impl.rs:152-159` — 缺 `-c core.hooksPath=/dev/null` 或 `GIT_CONFIG_NOSYSTEM=1` |
| **YAML 解析失败静默降级到行解析器，不报错** | LOW | `frontmatter.rs:34-57` — 畸形 YAML（如缺失引号闭合）可能产生错误元数据 |
| **激活分数阈值 0.35 硬编码且不可配置** | LOW | `activation.rs:90` |
| **无定期自动更新机制，仅手动触发** | LOW | `skill_install_service.rs` — 无后台轮询 |
| **前端 SKILL.md 编辑器无语法校验或保存前预览** | LOW | `SkillsPanel.tsx:692-718` |

---

### 2.6 网络检索能力 — 评分：A-

**做得好的：**
- 双搜索引擎：MiniMax API（主） + DuckDuckGo Instant Answer / HTML Search（备），含自动回退
- 页面抓取采用 Readability 语义内容提取（优先 `main/article` 标签，回退去噪）
- SSRF 防护全面：禁止 IP 直连（IPv4/IPv6）、内网地址检测（10.x/172.16/192.168/ULA 等）、DNS 重绑定检测（`.local`/`.internal` TLD）
- 搜索缓存（6h TTL，SHA-256 键）和网页缓存（24h TTL，URL 键），含速率限制（2s/请求）
- 页面抓取含 15s 超时、2MB 大小限制、24,000 字符提取上限

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **证书固定（Certificate Pinning）虽以模块命名但明确未实现** | MEDIUM | `cert_pinning.rs:10` — 注释写明 "无证书固定"，面对 CA 妥协攻击无防护 |
| **DNS 重绑定检测不覆盖非点分隔格式** | LOW | `fetch_web_page.rs:138-168` — 如 `10-0-0-1.example.com` 可绕过点分隔检测 |

---

### 2.7 文件读写权限设置及读写能力 — 评分：A-

**做得好的：**
- `.iris/` 和 `.classified/` 路径在所有读写工具中被一致拦截（7+ 处调用 `is_user_note_path()`）
- 外部文件导入/导出含系统敏感路径黑名单（`/etc/`, `/usr/`, `/var/`, `C:\Windows\` 等），macOS 临时目录例外放行
- 原子写入模式（`.tmp` + `rename`），失败时安全删除临时文件
- 路径遍历攻击通过正则化 + 前缀检查双重防护
- 后端链接源过滤掉 `.classified/` 路径

**存在的问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **无 OS 级别沙箱（seccomp/chroot/namespace）** | MEDIUM | N/A — 纯应用层路径验证，无内核级隔离 |
| **暴力破解保护状态仅存于内存，进程重启即重置计数** | LOW | `brute_force.rs:14` |
| **安全删除仅为单次覆盖，承认 SSD 磨损均衡限制** | LOW | `secure_delete.rs:27` |

---

### 2.8 Agent 权限设置安全性 — 评分：C

**这是整个系统最大的架构性问题。**

**做得好的：**
- 58个细粒度权限原子定义完整（`agent_permissions.rs:15-130`），含风险级别（Low/Medium/High/Critical）和作用域（Request/Session/Vault/Folder/Skill/Global）
- 权限授权持久化到 `agent_permission_grants` 表，支持过期时间
- 审计日志写入 `agent_permission_audit` 表
- 前端类型完美镜像 58 个后端原子（`types/ai.ts:35-90`）

**严重问题：**

| 问题 | 严重度 | 位置 |
|------|--------|------|
| **权限模型只在任务恢复时检查，主线执行路径完全忽略它** | **CRITICAL** | `agent_permissions.rs` vs `tool_executor.rs` — 实际工具执行门控使用的是 `tool_policy::evaluate_tool()`（能力匹配+自主级别），**不是** `agent_permissions` 的 58 个权限原子。`permission_has_active_grant` 仅在 `ai_commands.rs:301-325` 的 resume 路径中被调用 |
| **`permission_profile_for_tool()` 仅用于 UI 展示，未参与执行门控** | **CRITICAL** | `agent_permissions.rs:335-437` — 在 `pause_for_tool_confirmation()` 中被调用但仅用于计算前端展示的 `permissionEffects`，实际放行由 `check_tool_policy()` 决定 |
| **`ToolPolicyVerdict` 的三个状态（AutoAllowed/RequiresConfirmation/Denied）与权限授权系统完全脱钩** | **CRITICAL** | `tool_policy.rs` — 策略引擎看不到 `agent_permission_grants` 表，用户对某工具授权"Always Allow"后，策略层仍可能要求确认 |

**实质：58个精细权限原子是"纸面架构"，在实际工具执行路径中不被强制执行。** 工具审批走的是旧的 `tool_policy` 层（自主级别 + 能力匹配），而非用户可配置的权限策略。

---

### 2.9 沙箱设置及运行情况 — 评分：D+

**几乎不存在真正的沙箱机制：**

| 方面 | 现状 | 评估 |
|------|------|------|
| OS 级沙箱（seccomp/chroot/namespace） | **无** | 高风险 Skill 或子进程无内核级隔离 |
| 进程隔离 | **无** | AI Agent 与主进程共享同一 tokio 运行时和内存空间 |
| 文件系统隔离 | 仅应用层路径验证 | 无虚拟化、无 chroot、无只读挂载 |
| 网络隔离 | HTTPS-only（rustls） + URL 验证 | 有基本防护，但缺证书固定和流量审计 |
| Git 子进程 | 有环境变量清除（`env_clear`） | 但缺 hook 抑制和配置隔离 |
| 证书固定 | **未实现** | 模块名叫 `cert_pinning.rs` 但注释写"无证书固定" |
| 安全删除 | 单次零覆盖 | 承认 SSD 磨损均衡限制 |

唯一的实际防护层是：路径验证 + HTTPS-only + 提示注入过滤。对恶意 Skill 或对抗性模型输出的防护主要依赖应用层策略。

---

### 2.10 Agent 内部整体运行协作能力 — 评分：B

**做得好的：**
- 模块依赖单向无循环：`ai_harness` → `ai_runtime` → `ai_types`，层次分明
- 完全遵循 AGENTS.md 规范——技术栈锁定、命名约定、IPC 类型安全
- 前后端类型同步完整：`src/types/ai.ts`（1149 行）镜像所有 Rust `ai_types` 结构体
- IPC 封装层完善：`src/lib/ipc.ts`（1364 行）不含任何直接 `invoke()` 调用
- 几乎所有模块有对应的单元测试文件（Rust `#[cfg(test)]` 模块 111 处 + 前端 `tests/` 目录）
- 无死代码——8 处 `#[allow(dead_code)]` 均有文档化理由（计划中功能）

**存在的问题：**

| 问题 | 严重度 | 说明 |
|------|--------|------|
| **缺少端到端集成测试（含模拟 LLM 的完整 Harness 循环）** | MEDIUM | Harness 循环仅在单元级测试（策略检查、工具分发），从未用真实模型响应跑完整的多次工具调用循环 |
| **并发子Agent 的竞态条件无测试覆盖** | MEDIUM | `join_all` 并行执行路径无并发正确性测试 |
| **前端断开连接时 Harness 继续运行，无心跳检测** | LOW | `run.rs` 中用 `let _ = app_handle.emit(...)` 推送事件，前端断开时静默失败；有检查点保存可恢复但非实时感知 |
| **`UnifiedAssistantPanel` 状态变量过多（20+ `useState`）** | LOW | 可能导致不必要的重渲染，但使用 `useCallback` 和 `useRef` 做了优化 |
| **`settings_reset` 命令在 `lib.rs` 注册但缺少前端 TS 封装** | LOW | `ipc.ts` 中无对应包装函数 |

---

## 三、按严重度排序的重点问题汇总

### CRITICAL（架构性缺陷）

| # | 问题 | 位置 |
|---|------|------|
| 1 | **权限模型未在主线执行路径中被强制执行** — 58个权限原子仅在任务 resume 时检查，实际工具门控用的是另一套策略系统（`tool_policy`），使精心的权限设计形同虚设 | `agent_permissions.rs` ⇄ `tool_executor.rs` |

### HIGH（功能缺陷 / 安全隐患）

| # | 问题 | 位置 |
|---|------|------|
| 2 | **长对话消息无上限** — 数据库无限增长，缺少 Token 感知的上下文窗口管理 | `session.rs:105-137` |
| 3 | **多确认工具只处理第一个，其余静默跳过** — LLM 一次调用多个需确认的写入工具，仅第一个弹出确认对话框 | `tool_turn.rs:9-29` + `run.rs:509-510` |
| 4 | **子Agent 在 depth >= 2 时被静默忽略** — LLM 不知道调用失败 | `run.rs:426` |

### MEDIUM（应尽快修复）

| # | 问题 | 位置 |
|---|------|------|
| 5 | `content_hash` 字段永远为 `None` | `session.rs:47` |
| 6 | 子Agent 不继承父Agent 已积累的证据（仅传 cold_start_packets） | `run.rs:1016` |
| 7 | 父Agent 中止不级联到并行运行中的子Agent | `run.rs:1008` |
| 8 | 证书固定未实现（模块命名误导） | `cert_pinning.rs:10` |
| 9 | Git clone 未禁用 hooks/smudge filters | `skills_impl.rs:152-159` |
| 10 | 非 Web/Search 工具失败零重试 | `tool_dispatch_impl.rs:58-79` |
| 11 | 无端到端集成测试（模拟 LLM 的完整 Harness 循环） | 全局 |
| 12 | Harness 检查点绕过脱敏验证 | `harness_support.rs:156-164` |
| 13 | Token 估算降级方案过于粗略 | `token_estimator.rs:23-33` |

### LOW（改进建议）

| # | 问题 | 位置 |
|---|------|------|
| 14 | `settings_reset` 命令缺前端 TS 封装 | `ipc.ts` |
| 15 | `retract_messages` 无 `from_seq` 下限校验 | `session.rs:325-340` |
| 16 | YAML 解析失败静默降级 | `frontmatter.rs:34-57` |
| 17 | 激活分数阈值 0.35 硬编码 | `activation.rs:90` |
| 18 | Skill 无定期自动更新机制 | `skill_install_service.rs` |
| 19 | 规则意图检测纯关键词匹配 | `context_planner.rs:180-203` |
| 20 | DNS 重绑定检测不覆盖非点分隔格式 | `fetch_web_page.rs:138-168` |
| 21 | 前端断开连接无心跳检测 | `run.rs` |
| 22 | 安全删除仅单次覆盖 | `secure_delete.rs:27` |
| 23 | 前端 SKILL.md 编辑器无校验 | `SkillsPanel.tsx:692-718` |

---

## 四、架构优势总结

1. **模块层次分明**：`ai_types`(共享类型) → `ai_runtime`(协调层) → `ai_harness`(执行循环) → `commands`(IPC桥接)，依赖单向无循环
2. **安全纵深防御**：提示注入过滤(5层) → 工具策略(4层) → 权限模型(58原子) → 网络隔离(SSRF防护)
3. **技能系统成熟**：OpenCode/Claude 兼容的技能规范，四种安装源，BM25+向量双阶段排名，工作区隔离含符号链接防护
4. **前后端类型安全**：1149行 TS 类型镜像所有 Rust 结构体，1364行 IPC 封装不含任何直接 `invoke()` 调用
5. **测试覆盖广泛**：Rust 内联测试 111 处 + 独立测试文件 + 前端合约测试
6. **完全遵循 AGENTS.md**：技术栈锁定、命名约定、commit 规范、IPC 接口契约全部合规

---

## 五、建议修复优先级

### 第一阶段（立即）
1. 将 `agent_permissions` 的权限检查接入主线工具执行路径（`check_tool_policy`）
2. 修复多确认工具仅处理第一个的 Bug

### 第二阶段（本周）
3. 为 `session_messages` 添加消息数量上限或自动归档机制
4. 修复子Agent 深度限制静默失败问题
5. 实现子Agent 证据继承（传递父Agent 已积累的 context packets）

### 第三阶段（本月）
6. 添加证书固定或至少移除误导性命名
7. Git clone 添加 hook 抑制参数
8. 添加端到端集成测试框架（含模拟 LLM）
9. 实现 Token 感知的上下文窗口滑动管理

---

*报告结束*
