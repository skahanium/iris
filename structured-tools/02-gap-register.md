# 02. 差距登记

状态定义：

- **Confirmed**：代码或实例数据直接证明缺口存在。
- **Partial**：安全结构存在，但生产覆盖不完整。
- **Resolved**：修复前失败、修复后通过，且软件与实例门禁满足对应边界。
- **Deferred**：明确不进入本专项，不得借 Deferred 放宽安全门。

## 1. P0 差距

| ID              | 状态      | 事实                                                            | 用户影响                                     | 最小弥补边界                                                          |
| --------------- | --------- | --------------------------------------------------------------- | -------------------------------------------- | --------------------------------------------------------------------- |
| DOM-AVAIL-001   | Confirmed | 当前实例 11 个领域 binding 全部为 0                             | 模型看似有工具，实际无法取得领域数据         | 为每个受支持 operation 配置并验证真实 binding；无 Provider 时隐藏能力 |
| DOM-HEALTH-001  | Confirmed | `fresh_domains/provider.rs` 的 `healthy` 集合不读取持久化健康表 | 持续超时 Provider 仍可能成为首选             | 统一 readiness/health 派生，resolver、snapshot 和 UI 共享             |
| DOM-SURFACE-001 | Confirmed | 五个工具共享粗粒度 `web.domain.read`                            | 一个天气 binding 可能让金融工具看似可用      | 工具表面、`capabilities_read` 和 dispatch 精确到 operation            |
| DOM-ROUTE-001   | Confirmed | capability resolver 找到任意领域 binding 即可解析能力           | binding 与请求领域错配，直到 dispatch 才失败 | intake 生成 operation-specific grant 并冻结匹配 snapshot              |
| DOM-LIVE-001    | Confirmed | 现有测试主要使用 fixture/模拟输出                               | 内部测试全绿但用户实例仍 0/11                | 分开建立受支持 operation 软件门禁（目标 11 个）和真实实例门禁         |
| DOM-EVID-001    | Partial   | DTO/evidence/Host 终局已有部分生产接线                          | 某些 operation 可能只通过组件级测试          | 每个 operation 从正式 intake 走到持久化最终消息并可恢复               |
| DOM-UPGRADE-001 | Confirmed | 安装版仍可能使用旧 binding schema                               | UI 无法保存 `web.domain.read` mapping        | 验证真实 059→072 升级、重复启动幂等和数据目录                         |

## 2. P1 差距

| ID               | 状态      | 事实                                                                                | 用户影响                                     | 最小弥补边界                                                |
| ---------------- | --------- | ----------------------------------------------------------------------------------- | -------------------------------------------- | ----------------------------------------------------------- |
| DOM-MGMT-001     | Partial   | 管理中心可以保存 mapping，但没有完整的受支持 operation readiness 总览（目标 11 个） | 用户不知道缺哪个服务，也无法判断是否真实可用 | 显示未配置、待验证、可用、降级、不健康及安全原因            |
| DOM-PREVIEW-001  | Confirmed | 保存 mapping 不等于真实响应能通过 DTO validator                                     | 错误字段映射会在用户提问时才暴露             | 保存后由用户显式执行受限真实预览，预览通过才 Ready          |
| DOM-FALLBACK-001 | Confirmed | 目标文档曾暗示多领域 Web fallback，生产只允许 News                                  | 认知与实现不一致                             | 按 operation 明确 fallback；非 News 不以普通 Web 冒充       |
| DOM-RETRY-001    | Partial   | 业务补搜、同 Provider 重试和备用 Provider 切换边界仍需生产证明                      | 可能多搜、重复调用或扩大预算                 | 技术尝试与业务搜索轮次分开计数并持久化                      |
| DOM-ERROR-001    | Partial   | Provider 不可用常投影为通用提交或执行错误                                           | 用户不知道要去哪里配置                       | 稳定安全码映射为可操作文案，不泄露参数或凭证                |
| DOM-DECISION-001 | Confirmed | operation 级 readiness/preview 没有持久化载体，与“不新增表”冲突                     | Ready 状态不可恢复，管理中心矩阵不可信       | 先关闭决策门 1：允许最小 schema 演进或明确 Ready 持久化依据 |
| DOM-PROVIDER-001 | Confirmed | 缺少 Provider 落地策略与强制决策流程，AI 可能自行选择 MCP/API                       | 施工可能卡在 Provider 接入或做出错误架构假设 | 每个 operation 必须先完成并确认 PDR（见文档 07）            |
| DOM-COVERAGE-001 | Confirmed | 多 Provider 覆盖同一 operation 的子集没有建模与路由规则                             | 部分覆盖被误认为完整可用                     | 必须建立覆盖矩阵，未覆盖范围只能是 Unavailable              |

## 3. 已有基线，不得重复施工

| ID               | 状态     | 已有事实                                          | 保持方式                        |
| ---------------- | -------- | ------------------------------------------------- | ------------------------------- |
| DOM-RUNTIME-001  | Resolved | `system_time_now` 使用本机时间，不需要 Web        | 保留日期问题不联网回归          |
| DOM-VALIDATE-001 | Resolved | 五领域 DTO 有字段、HTTPS 来源和时效 validator     | 保留必需字段和陈旧数据负例      |
| DOM-SEC-001      | Resolved | output mapping 受限，原始 Provider 输出有诊断哨兵 | 新预览和 readiness 不能绕过脱敏 |
| DOM-INTAKE-001   | Resolved | 0 个结构化候选不再误判为 Provider 歧义            | 保留第三轮当前电影 intake 回归  |

## 4. 技术债控制原则

- readiness 必须是现有 binding、Provider、health 的只读派生，禁止新增第二套真相源；是否允许最小 schema 演进（表/列/migration）由决策门 1/3 确认，未确认前不新增。
- operation 是最小授权和路由单位，禁止用领域名或 `web.domain.read` 粗粒度替代。
- 普通 Web mapping 与领域 mapping 永久分离。
- Provider 真实输出只在内存中完成 mapping/validation，不持久化原始 JSON。
- 实例未配置属于正常可观测状态，不通过自动创建虚假 binding 来“修复”。
- 找不到合规 Provider 时缩减支持声明，不放宽验证合同。
