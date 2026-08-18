# 02. 从当前实现到最小目标形态

本次重构不推倒重建。目标是复用现有 Run、事件、工具目录、证据表和会话摘要，在真实断点处补齐契约。

## 1. 总体目标形态

运行时只增加一个代码层面的 `RunSituation` 投影，不新增持久化实体：

```text
ExecutionEnvelope
  + 已提交的会话消息
  + 当前 Run 事件与工具结果
  + 当前 Run 的 session_evidence
  + 冻结的 intake / tool surface
  + 必要时的 conversation_summary
  = RunSituation（只读投影）
```

Executor 只消费这份冻结输入；UI 只消费已持久化状态和可重放事件。这样可以减少“路由器认为可做、模型看到可做、执行器实际不能做”的分叉。

## 2. Run 生命周期

### 当前差距

- retry 仓储层可以判断请求是否已被接受，但上层丢失 `is_new` 信息，仍可能再次启动执行器。
- `AnswerComplete` 可能在最终助手消息持久化之前发出，持久化失败时 UI 已看到成功。
- 用户拒绝确认、事件下发失败和恢复后的终态投影缺少统一说明。

### 目标调整

- 内部 accept/retry 返回 `{ accepted, is_new }`；公共 IPC 若无兼容性需要则不改签名。
- 仅 `is_new=true` 时启动执行器，并为普通会话增加单航班约束。
- 最终化顺序固定为：校验证据 → 持久化助手消息与绑定 → 持久化 Run 终态 → 发出 `AnswerComplete`。
- 事件 sink 失败只影响实时展示，恢复时从持久化快照/事件补齐。
- 用户拒绝统一映射为现有 `Cancelled`，避免扩张状态枚举。

## 3. Intake、路由与工具表面

### 当前差距

- intake、能力分类、模型可见工具和实际执行之间仍存在重复判断。
- `capabilities_read` 读取完整目录，而不是当前 Run 真正可用的工具表面。
- 过早引入额外 LLM Router 会增加延迟、成本和新的不一致点。

### 目标调整

- intake 只做确定性分类、权限快照和输入规范化，结果随 Run 冻结。
- 首阶段不新增 LLM Router；只在确定性规则无法覆盖且有评测证据时再考虑。
- 复用正在形成的 `ToolSurfacePlan`：让 `capabilities_read`、提示词暴露和执行门禁都读取同一份冻结计划及其已解析工具列表，不再新增一套 snapshot 类型。
- Web 开关、模型能力、工具实现状态和确认策略共同裁剪表面，但任何模型输出都不能扩大用户授权。

## 4. 工具目录与执行

### 当前差距

- `ToolImplementationStatus` 已能区分 `Dispatchable`、`HarnessOnly`、`Planned`，但权限、能力读取和执行映射仍有旁路或错误映射。
- `spawn_subagent`、`conclude_reasoning` 等遗留入口与当前个人笔记产品的真实用途不匹配，部分参数没有生产消费方。

### 目标调整

- 保留现有 `ToolCatalogEntry` 和 `ToolImplementationStatus`，不新增第二套成熟度枚举。
- 仅在现有目录上补充确有执行用途的 `cost_class`、`output_policy`、`evidence_policy` 等静态元数据。
- 工具展示、参数校验、权限校验和 dispatch 共享目录事实；执行前仍由单一门禁最终裁决。
- 修正错误的权限映射；无真实调用链的 reasoning/subagent 工具直接删除或保持不暴露，不为其预建平台能力。
- 删除未被消费的工具参数，避免“模型以为有作用、运行时实际忽略”。

## 5. Web 证据与来源展示

### 当前差距

- ToolLoop 的未校准路径已经能生成 `SourceGroupFallback`。
- Direct 严格 Web 路径在最终化时没有生成 citation binding，因而绕过该 fallback。
- UI 只有在显式收到 fallback 时才显示“本次检索来源/未逐段核验”；绑定缺失时仍可能以普通“来源”呈现。
- 严格结构化 VERIFIED 规则尚未形成有效覆盖。

### 目标调整

- 所有 Web 最终化路径显式生成 `Exact`、`Normalized` 或 `SourceGroupFallback` 之一。
- Direct 路径在无法生成精确绑定时必须生成 fallback，不能传空值。
- UI 对缺失、未知或解析失败的绑定采取 fail-safe：按来源组展示，并标注未逐段核验。
- 只有确定性、可复现的结构化校验能晋升 VERIFIED；无规则时保持 uncalibrated。
- 来源排序只有在真实可靠时才显示；不得把插入顺序包装成质量排名。

## 6. 上下文压缩与最小记忆

### 当前差距

- `conversation_summaries` 已存在，但运行时上下文投影的生产接入不完整。
- 兜底逻辑可能把第一条用户消息直接提升为目标，造成陈旧目标持续影响后续 Run。
- `ai_memories` 已存在，但键唯一性和作用域语义不足以支撑安全的少量偏好记忆。

### 目标调整

- 先接通 `RunSituation`：短会话直接使用已提交消息，超过预算才加载或生成摘要。
- 摘要记录覆盖范围和生成依据；新消息、删除消息或模型切换需要触发失效检查。
- 删除“第一条用户消息永远是当前目标”的兜底，当前目标只能来自本次请求或明确的活跃任务状态。
- `ai_memories` 使用 `(scope, key)` 唯一性，提供按 scope 清理；仅写入用户确认的偏好。
- Web 结果、模型猜测和本地检索片段一律不能自动写入长期记忆。

## 7. 前端投影

- 复用现有过程事件和状态组件，不再引入独立 Harness 仪表盘。
- `capability_degraded` 若已有组件但生产链未接入，应补接入与测试，而不是重复造组件。
- 只展示能由持久化事实恢复的关键状态：等待确认、工具执行、降级、取消、失败和完成。
- 不恢复原始工具参数的默认展示；必要诊断信息必须经过脱敏和结构化摘要。
