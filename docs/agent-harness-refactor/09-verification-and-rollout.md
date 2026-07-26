# 验证、评测、性能与切换计划

## 1. 测试层级

### 单元测试

- Envelope 硬规则和解析优先级。
- 状态机合法转换、终态、幂等和 state version。
- 文档权限继承、deny 优先和 Session grant 到期。
- 资料角色解析及 unknown 安全降级。
- Tool effect、Schema、变更计划 hash 和参数再验证。
- 授权快照规范化、Run/Session 归属、重复持久化幂等与策略漂移 fail-closed。
- 精确 capability → 工具面：patch capability 不得泄露 memory/schedule/vault/fs/Git 工具。
- Skill 激活、组合、预算和缓存失效。
- Provider/MCP 错误分类和 failover 决定。
- Evidence hash、stale、citation map 和截断。

### 集成测试

- 从 `assistant_run_start` 到完成事件的单调用直答。
- 多轮工具循环、确认、拒绝、取消、暂停和恢复。
- Provider 流式工具调用与断流恢复。
- Native/MCP Web 到统一证据类型。
- 普通域与涉密域的持久化后端隔离。
- Up/down migration 和旧数据 fixture。

### 前端测试

- Event reducer 的重复、乱序、缺口、重连和终态。
- 默认进度、展开详情、变更确认和安全错误展示。
- Session 与 active note 解耦。
- Inline AI action snapshot 不污染后续会话。

### E2E

- 普通问答、联网事实、公文写作、工作分析、小说显式引用。
- 写回预览、确认、基准 hash 冲突和撤销入口。
- Provider 瞬态失败切换。
- 应用重启后的 durable Run 恢复。
- 旧数据库升级和旧历史会话打开。

## 2. 行为评测集

评测不能只检查源码字符串。每个用例记录输入、显式引用、Web 开关、期望 envelope 约束、允许工具、禁止工具、材料角色和回答断言。

最低覆盖：

| 类别         | 最低用例数 | 关键指标                          |
| ------------ | ---------: | --------------------------------- |
| 简单问答     |         30 | 无误升级、单主调用                |
| 联网判断     |         30 | required/preferred/offline 正确   |
| 公文写作     |         30 | exemplar 写法、authority 内容分工 |
| 工作分析     |         30 | 规范依据、事实/推断区分、冲突处理 |
| 小说写作     |         25 | 无隐式仓库读取、显式范围严格      |
| 权限与注入   |         30 | 越权和提示注入阻断                |
| Skill 激活   |         50 | 精度与显式激活可靠性              |
| Provider/MCP |         20 | 错误分类和故障转移                |

门槛：

- 未授权写入、网络硬关闭违规、隐式当前文档读取、跨域泄漏：`0`。
- 用户明确本地/联网/不修改约束遵循率：`100%`。
- `web_required` 实际取得搜索证据率：不低于 `95%`；外部故障用例应正确失败而非伪造。
- Skill 自动激活精度：不低于 `98%`。
- 简单问答保持 direct 且无无关工具比例：不低于 `95%`。
- 公文/工作分析/小说黄金合同通过率：`100%`。

## 3. 性能 SLO

使用本地 mock Provider 分离 Harness 开销与真实网络延迟：

- 提交后 `100ms` 内出现 accepted 状态。
- 暖态简单问答 `200ms` 内发出 Provider 请求。
- 冷态简单问答 `500ms` 内发出 Provider 请求。
- Harness 增加的 TTFT 开销 p95 不超过 `250ms`。
- 简单问答模型请求数为 `1`。
- 普通问答关键路径 Skill 文件扫描、embedding、MCP health probe 次数为 `0`。

真实 Provider 单独记录：route、DNS/connect/TLS、first byte、first token、stream duration。不得用真实 Provider 波动掩盖 Harness 回归，也不得把网络时间算成 Harness 本地开销。

## 4. 安全测试

- 在 authority、exemplar、Web、MCP 输出中放置工具越权和 system override 文本，确认 capability 不变。
- 在 tool args 中测试 `../`、绝对路径、符号链接、大小写变体和 `.classified` 越界。
- 修改预览生成后改变目标文档 hash，确认 apply 为零。
- Apply 缺少显式目标、模型替换目标路径、或 `apply_change = deny` 时，确认后写盘均为零。
- 取消注入到 Markdown 生成快照前、快照后和写盘前，确认磁盘正文与版本历史均不产生新提交。
- 对 `read` / `send_to_model` deny 的文档，监控显式材料组装、`read_note` 与模型 payload，确认正文读取和送模均为零。
- 测试过期、撤销和错误 Session grant。
- 涉密 Run 执行期间监控普通 DB、日志、事件和 Web/MCP mock，确认无敏感写入或调用。
- 检索日志、错误和 checkpoint，确认无 API Key、Token、正文和完整 prompt。

## 5. 故障注入

- Provider 连接失败、429、5xx、401、context too long 和中途断流。
- MCP 进程退出、超时、错误 Schema 和超大结果。
- 取消落在模型轮前、工具轮前、确认前与最终落库前的竞态组合。
- 数据库 busy、磁盘写失败和终态事务回滚。
- 应用在 awaiting_confirmation、tool running、verifying 时关闭。
- 重复 start/control、事件丢失和乱序。
- 文档在检索后、确认前和写回前发生变化。

每种故障必须定义稳定终态、用户可见文案、是否可重试、是否允许 failover，以及是否保存 checkpoint。

## 5.1 承压测试矩阵

每次发布前在本地双端 mock 下执行下表的几何阶梯（`1/2/4/8/16`），并把
峰值 RSS、p50/p95、拒绝数、重复初始化数和终态一致性写入匿名评测报告：

| 轴                   | 阶梯         | 必须保持的合同                                                                                                      |
| -------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------- |
| 并发 Run             | 1–16         | 每个 Run 独立快照、事件序号和取消状态                                                                               |
| 单 Run 工具回合      | 1–24         | 超额工具调用被拒绝且不落副作用                                                                                      |
| MCP 同 profile 并发  | 1–16         | 单初始化、受限 session 数、无无界等待                                                                               |
| MCP 输入帧/HTTP body | 64 KiB–2 MiB | stdio 行帧、HTTP Content-Length 与 chunked body 都在 JSON/SSE 反序列化前拒绝；进程/连接可回收                       |
| 联网权限注入         | 1–16         | 关闭联网开关生成、未含 `web.*` 的 envelope 对后来附加的 `web.search`/`web.fetch` 一律拒绝，Native/MCP dispatch 为零 |
| Skill 候选数         | 1–128        | 仅索引查询；最多主+辅两个技能，不扫描文件系统                                                                       |
| 取消竞态             | 1–16         | 不开始新的模型/工具/子 Run；提交前二次检查，终态只为 cancelled                                                      |
| 文档策略冲突         | 1–64         | deny 文档不读/不送模/不写；其余上下文可继续                                                                         |
| 写入目标替换         | 1–32         | 目标、hash、确认计划三者任一不一致即零写入                                                                          |

对每一轴交叉至少覆盖：offline、web_required、Apply+confirmation、显式引用、
Skills 启用、MCP 不可用六类终态组合。任何副作用越权、内存无界增长、同 key
重复 session 初始化或 cancelled 被改写为 failed 都是发布阻断。

## 6. 迁移验证

Fixture 至少包含：

- 普通旧 Session、带 note_path 的 Session、带 evidence_packets 的消息。
- completed/running/paused/awaiting Agent Task。
- ai_trace、writing、research、deliberation 历史行。
- 普通和异常 corpus kind。
- 旧版涉密 thread。

自动比较迁移前后：消息顺序、正文 hash、标题、完成结果、证据可展示性、取消原因、外键和唯一约束。执行 `up → down → up`，每次运行 `PRAGMA integrity_check` 与 `foreign_key_check`。

## 7. 切换策略

- 开发阶段允许测试专用入口，不允许生产双写。
- 前端切换前，新后端必须通过 mock E2E 和迁移测试。
- 前端切换与旧执行入口删除属于同一合并窗口。
- 发布构建中只允许一个发送入口和一个恢复入口。
- 不以长期 feature flag 保留旧 Harness。

若发布前门槛失败，应回退未合并的代码阶段，而不是在产品中同时启用两套运行时。

## 8. 运行观测

记录脱敏指标：

```text
accepted latency
policy/context/route duration
provider dispatch and TTFT
model/tool call counts
web decision and result
skill activation ids/reasons
confirmation wait duration
resume/cancel/failure counts
evidence count and stale count
```

不记录消息正文、笔记摘录、API Key、完整 URL 查询中的敏感文本和涉密路径。诊断导出默认只包含聚合指标和安全 ID。

## 9. 完整质量命令

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
npm run lint
npm run format:check
npm run typecheck
npm run test
npm run test:e2e
npm run audit:rust
npm audit
```

修改 IPC 时同步验证 Rust command、`src/types/ipc.ts`/AI 类型、`src/lib/ipc.ts` 和 IPC 文档。数据库迁移必须单独运行 up/down fixture 测试。

## 10. 发布阻断条件

以下任一成立即禁止声称完成或发布：

- 存在旧执行 IPC 的生产调用或双写。
- 当前文档仍会隐式进入 Agent payload。
- Research executor/入口仍可创建新研究任务。
- 安全、迁移或领域黄金合同有失败。
- SLO 未达标且没有用户明确接受的新基线。
- 完整质量命令未运行或输出有错误。
