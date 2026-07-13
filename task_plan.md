# Agent Harness 重构执行计划

## 目标

逐项对齐 `docs/agent-harness-refactor/` 的最终态：一个 Conversation/Run/Policy/Evidence 体系，移除旧执行入口、场景/当前文档隐式耦合与 Research executor，并完成可回滚数据迁移、测试、SLO 与发布门禁验证。

## 阶段

| 阶段                                       | 状态   | 内容                                                                   |
| ------------------------------------------ | ------ | ---------------------------------------------------------------------- |
| 1. 文档审计与差距矩阵                      | 已完成 | 已生成审计报告，确认新旧体系并存及全部阻断项。                         |
| 2. 新 Run 入口基础修复                     | 进行中 | 统一事件通道，移除新路径的 scene/note 条件与 legacy Chat intent 输入。 |
| 3. 涉密会话 CEF v2                         | 已完成 | 无 document_path、包含 Turn/Run/Event/Evidence、惰性迁移与原子替换。   |
| 4. 统一 Session API 与前端切换             | 进行中 | 建立 `assistant_session_*` 并替换 normal/classified 双入口。           |
| 5. Envelope/Policy/Capability/Run 生命周期 | 待开始 | 完整解析、权限、工具、确认、恢复、证据账本。                           |
| 6. 旧执行链与 Research 删除                | 待开始 | 删除 IPC、Rust executor/state、前端 hooks/types/UI。                   |
| 7. SQLite copy-transform-swap 与 down      | 待开始 | 最终表结构、fixture up/down/up、完整性验证。                           |
| 8. 验证、评测、SLO 与发布门禁              | 待开始 | 单元/集成/E2E、安全、故障注入、评测与全质量命令。                      |

## 关键约束

- 不修改用户 Markdown 笔记；不新增依赖；不使用 `unsafe`。
- 未完成旧入口删除、迁移、评测和完整质量门禁前，严禁声称整体完成。
- 新代码不允许依赖 scene、intent、notePath 或当前编辑器状态作为 Run/Session identity。

## Errors Encountered

| 错误                                      | 根因                                | 处理                                        |
| ----------------------------------------- | ----------------------------------- | ------------------------------------------- |
| Cargo 临时 target 锁被 sandbox 拒绝       | 构建目录不在普通 sandbox 可写锁范围 | Rust 测试使用批准的临时 target 与升级权限。 |
| 提升权限命令忽略 workdir                  | 执行环境行为                        | Git 使用 `git -C D:\Iris`。                 |
| 计划技能文件首次读取被拒绝                | sandbox 不允许读取技能目录          | 已通过升级权限读取技能说明。                |
| classified retract 编译借用冲突           | 保存了消息引用后又原地裁剪 Vec      | 改为先收集 ID/计数，回归测试通过。          |
| assistant command 缺少 run-contract 导入  | 多行替换未命中实际 import           | 精确补充 import，编译测试通过。             |
| 读取错误的重构文档文件名                  | 按语义猜测文件名，未先枚举目录      | 已用                                        |
| g --files 定位真实 01–10 文档后继续阅读。 |

| Rust 源码出现字面 ` 
` | 单引号 PowerShell 替换不展开换行 | 已替换为真实换行后再运行格式化与测试。 |
