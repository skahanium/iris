# 模块索引

<!-- iris:object README-MODULES kind=rules file=true -->

本目录是 9 个运行时模块与 1 套评测系统的正式定义位置。

| 模块  | 名称             | 组件        | 文件                                                                       |
| ----- | ---------------- | ----------- | -------------------------------------------------------------------------- |
| `M01` | 会话与 Run 管理  | `C01`–`C03` | [m01-session-and-run.md](./m01-session-and-run.md)                         |
| `M02` | 权限与受控副作用 | `C04`–`C06` | [m02-permission-and-side-effects.md](./m02-permission-and-side-effects.md) |
| `M03` | 上下文与记忆     | `C07`–`C09` | [m03-context-and-memory.md](./m03-context-and-memory.md)                   |
| `M04` | 模型网关         | `C10`–`C12` | [m04-model-gateway.md](./m04-model-gateway.md)                             |
| `M05` | Agent 执行控制   | `C13`–`C15` | [m05-agent-execution.md](./m05-agent-execution.md)                         |
| `M06` | 工具与连接器     | `C16`–`C18` | [m06-tools-and-connectors.md](./m06-tools-and-connectors.md)               |
| `M07` | 检索与证据       | `C19`–`C22` | [m07-retrieval-and-evidence.md](./m07-retrieval-and-evidence.md)           |
| `M08` | 验证与交付       | `C23`–`C25` | [m08-verification-and-delivery.md](./m08-verification-and-delivery.md)     |
| `M09` | 审计与诊断       | `C26`–`C27` | [m09-audit-and-diagnostics.md](./m09-audit-and-diagnostics.md)             |
| —     | 独立评测系统     | `E01`–`E03` | [eval-system.md](./eval-system.md)                                         |

组件合同写在所属模块文件内：模块文件前言的九项（输入与输出、决定权、状态、不变量、异常与恢复、审计、源码落点、兼容、测试）是该模块的正式定义；`C*` 子对象块是各组件的正式定义。**一个对象只有一个正式定义位置**，其他文档只引用。

本目录不声明实现完成：每份文件的 `implementation.state` 只描述静态源码事实，`verification.state=none` 表示没有绑定指纹的证据记录。

<!-- iris:end README-MODULES -->

<!-- iris:object README-MODULES kind=rules name=modules-index -->

### README-MODULES 目录索引

本对象是目录索引容器：登记该目录各对象的身份与职责边界，并声明**它不承载任何对象的正式定义**——每个对象的定义在各自的文件里。维护规则见 [rules/governance.md](../rules/governance.md)；对象身份与标记规则见 [rules/objects.md](../rules/objects.md)。

<!-- iris:end README-MODULES -->
