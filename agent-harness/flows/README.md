# 链路索引

<!-- iris:object README-FLOWS kind=rules file=true -->

本目录是 6 条链路的正式定义位置。链路只说明**交接顺序**并引用合同，不重复定义合同、组件或工具。

| 链路  | 名称           | 责任模块 | 承接需求           |
| ----- | -------------- | -------- | ------------------ |
| `L01` | 对话与修订     | `M05`    | `N01`、`N05`       |
| `L02` | 联网检索与核实 | `M07`    | `N06`–`N11`、`N13` |
| `L03` | 编辑与候选应用 | `M08`    | `N02`、`N03`       |
| `L04` | 格式保持       | `M08`    | `N04`              |
| `L05` | 上下文与记忆   | `M03`    | `N14`              |
| `L06` | 委派           | `M05`    | `N16`              |

链路之间的边界：`L02` 只覆盖「发起网页搜索之后」的执行；纯编辑任务（`L03`、`L04`）不因联网开关打开而自动检索。

<!-- iris:end README-FLOWS -->

<!-- iris:object README-FLOWS kind=rules name=flows-index -->

### README-FLOWS 目录索引

本对象是目录索引容器：登记该目录各对象的身份与职责边界，并声明**它不承载任何对象的正式定义**——每个对象的定义在各自的文件里。维护规则见 [rules/governance.md](../rules/governance.md)；对象身份与标记规则见 [rules/objects.md](../rules/objects.md)。

<!-- iris:end README-FLOWS -->
