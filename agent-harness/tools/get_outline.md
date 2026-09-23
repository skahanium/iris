# get_outline

<!-- iris:object T08 kind=rules file=true -->

`get_outline` 是 Iris 的内置业务工具之一（通用能力目录，共 39 个）。它是能力接口，不是权限：能否调用由 `C04` 授权决定，实际派发由 `C17` 负责。

<!-- iris:end T08 -->

<!-- iris:object T08 kind=tool name=get_outline owner=M06 -->

## 用途与语义归属

读取目标大纲：提取指定笔记的 Markdown 标题层级（ATX 与 Setext），给出每级标题及其在正文中的精确字节范围。语义归属为工具（`M6` / 文档服务），责任模块 `M06`。

它解决的是「先看结构、再定点读」：模型用大纲决定该读哪一段，然后用 `T06` `read_note` 的 `start_byte` 续读对应区域，而不是整篇灌入上下文。它不评估内容、不排序、不判断哪一节重要。

## 参数与消费

| 参数   | 类型   | 声明边界     | 处理器行为            |
| ------ | ------ | ------------ | --------------------- |
| `path` | string | **required** | 缺失即 `missing path` |

返回 `{ "path": <path>, "headings": [ { "level", "text", "sourceSpan": { "start", "end" } } ] }`。

源码事实：

- `sourceSpan` 的单位是 **UTF-8 字节**，可直接作为 `T06` 的 `start_byte` 使用；对 Setext 标题，span 覆盖「标题行 + 下划线行」。
- 解析与索引共用同一实现（`indexer::chunker::markdown_headings`，注释：「与索引相同的围栏与偏移规则」）：围栏代码块内的伪标题、以及空标题文本、缩进超过 3 空格的伪 Setext 标题都不计入。因此大纲与检索分块对「什么是标题」的判断一致。
- 无分页参数：一次返回全部标题，没有 `limit`／`max_chars`。超长笔记的大纲体积**没有声明上限**，这是本卡记录的**待核对**项（是否需要有界化，尚未登记为 `G*`）。

## 授权

- 能力合同（`C16`）：`required_capability_ids() = ["vault.read"]`；`access_level = ReadNoteSpan`。
- 权限原子（`C04`）：`Atom::VaultRead`，Low，`supported = true`，`requires_confirmation = false`。
- 与 `T06` 相同的四道边界，且顺序一致：① 文档策略 `Discover`／`Read`／`SendToModel` 三项逐一判定；② `retrieval_scope` 允许该路径；③ 活跃技能作用域允许该路径；④ 路径为 vault 内合法用户笔记（拒绝绝对路径、`..` 穿越、`.iris/`、`.classified` 及规范化后的别名）。
- 不受联网开关约束。子任务继承同一授权与范围，不能借大纲读取范围外笔记。

## 副作用

**无文件写入、无数据库写入。** 处理器一次 `std::fs::read_to_string` 加纯解析，不写索引、不改配置、不落证据正文。输出策略 `bounded_note_span`，证据策略 `current_run_local`（`LOCAL_NOTE_SPAN`）。

## 预算

- `cost_class = "local"`，归入 `ToolBudgetClass::Local`（当前预设 `max_local_tool_calls = 12`、`max_tool_calls = 24`；主循环分类额度，不是全局统一总数；数值权威在 `K06`）。
- 无 `max_results`（`None`）与内容上限声明——见「参数与消费」的待核对项。
- **不进发现调用配额**（`is_discovery()` 不含本工具）：读结构不算发现动作，不受每回合 2 次限制。
- 派发包裹在 30 秒超时中；不在自动重试白名单。

## 幂等

纯读取，重复调用不产生副作用。结果随文件内容变化：同一路径在编辑后大纲不同，且 `sourceSpan` 会整体位移。因此 span 只能与**同一次读取**的正文版本配合使用；跨调用复用旧 span 需要重新读取或核对版本（`T06` 的 `content_hash` 可用于该核对）。

## 取消

派发前执行 `ctx.ensure_run_active()`；已取消的 Run 不打开文件。读取与解析同步执行，无执行中途取消检查点。不留下部分结果。

## 失败反馈

- 参数无效：缺少 `path` → `missing path`（字段级说明，方向与 `G01` 一致）。
- 边界拒绝：路径越界、范围外、技能作用域外、文档策略拒绝分别返回不同错误文本；均**不可通过重试同一参数恢复**，应改目标或告知用户。
- 正文不可读：IO 错误经派发层成为失败的工具结果；空大纲（无标题笔记）是**成功**结果，`headings` 为空数组——这一点与「读取失败」在模型侧可区分，须如实保留差别。
- 超时：`{"error":"tool_dispatch_timeout","failure_class":"timeout", ...}`。
- 用户侧：单次失败不得表达为「模型能力降级」（`N17`）。

## 暴露规则

- `default_enabled_without_skill = true`。
- 目标工具面：对话允许笔记库读取时「基础加六个通用本地工具」之一；也是无 Range 的显式引用场景允许清单成员（该清单含 `get_outline`）。
- 需要 Run 冻结能力含 `vault.read`。
- 无 Planned 状态。

## 源码落点

- 目录定义：[tool_catalog/read.rs](../../src-tauri/src/ai_runtime/tool_catalog/read.rs)（第 143 行起，`Dispatchable`、`ReadNoteSpan`、`max_results = None`）。
- 派发与处理器：[tool_dispatch_impl.rs](../../src-tauri/src/ai_runtime/tool_dispatch_impl.rs)、[tool_dispatch/note.rs](../../src-tauri/src/ai_runtime/tool_dispatch/note.rs)（`get_outline`）。
- 解析实现：[indexer/chunker.rs](../../src-tauri/src/indexer/chunker.rs)（`markdown_headings`、`MarkdownHeading`：level、text、`source_start`、`source_end`；ATX + Setext、围栏状态与偏移规则）。
- 边界与范围：[tool_dispatch/context.rs](../../src-tauri/src/ai_runtime/tool_dispatch/context.rs)、[storage/paths.rs](../../src-tauri/src/storage/paths.rs)（`validate_user_note_relative_path`）、[retrieval_scope.rs](../../src-tauri/src/ai_runtime/retrieval_scope.rs)。
- 现有测试：[tool_dispatch/tests.rs](../../src-tauri/src/ai_runtime/tool_dispatch/tests.rs)（`get_outline_uses_markdown_heading_ranges_and_ignores_fences`、`get_outline_rejects_iris_metadata`）。

## 当前状态

`implementation.state = present`（静态源码事实：目录项 `Dispatchable`，处理器与共享解析实现均在位）。`verification.state = none`。

不确定处（**待核对**，尚未登记为 `G*`）：大纲无体积上限；`sourceSpan` 与 `read_note` 的 `start_byte`／`content_hash` 的组合使用在真实多轮任务中的正确性本轮未验证。未执行真实模型验收，不声明合同正确或用户任务可用。

## 相关合同

- `K05` 授权范围与冻结确认：范围来自冻结授权；本卡只引用。
- `K10` 工具面版本与派发观察。
- `K06` 统一预算账本：Local 分类额度的唯一权威。
- 相邻工具：`T06` `read_note`（用大纲 span 定点续读）、`T07` `list_vault`（先列举路径）、`T05` `search_hybrid`（按内容命中）。
- 需求依据：`N01`（基础对话可靠）、`N14`（承接已授权材料时的上下文经济）；「读取目标大纲」的取舍见架构定义 §6.2。

<!-- iris:end T08 -->
