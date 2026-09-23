# Iris 前端 / Markdown / Agent 衔接审计报告

> 审计日期：2026-09  
> 审计范围：前端建设、Markdown 语法高质量实现、Agent 功能衔接，以及与 [Agent 架构定义](./agent-architecture.md)、[agent-harness](../agent-harness/README.md) 的偏差。  
> 证据口径：源码与测试静态核对；**未运行付费实网评测，未做桌面 E2E**。工作包 `done/active` 引用登记表自述状态，不等于产品放行。  
> 复核：经外部复核（GPT）逐条核对高风险路径；**已采纳的修正与撤回项见 §2**，有效问题见 §3–§5，排期建议见 §6。原稿中被复核推翻的结论不再作为施工依据。

---

## 一、总结论

运行底座（Run 生命周期、冻结确认、hash 防写穿、证据账本、诊断）扎实。真正的问题集中在三处：

1. **数据保真**：显式保存会静默改写用户 `.md`（脚注编辑丢失、加粗 repair 注入/去转义、表格对齐丢失）。
2. **表达与合同**：部分 UI 文案与已确认需求（N09/N17）有张力；「用户接受 AI 候选」与 Agent Apply 的边界合同未写清。
3. **未完成验收**：双路搜索（N07）完整逐端点 V05 未完成；格式整理内容保持（N04）对表格/HTML 不能自证。

---

## 二、经复核修正的原结论（不再作为施工依据）

| 原结论                                     | 复核结果                       | 说明                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------ | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Inline/Slash AI 绕过 C06，构成双权威硬冲突 | **定性过重，降级为合同澄清项** | 架构明确 C06 不接管非 Agent 编辑器全部保存；Inline AI 以 `effect: "draft"` 生成候选，用户「接受」后按普通编辑处理。残余问题仅两点：①「用户接受 AI 候选」归属（C24 / draft 终态 / 纯编辑器）未写清；②与 Agent Apply 并行触达同一文件时，§8.5「文件意外修改」诊断链覆盖不到。**先澄清合同，不按写入漏洞施工**。                                                                    |
| 原生不支持、单次工具失败都会弹降级黄条     | **触发条件错误**               | `native_unsupported_is_normal_single_mcp_without_degradation_copy` 证明原生不支持走正常单 MCP、不发降级。真实条件：Web 尝试失败**且**本 Run 无可用 Web 证据时才发 `capability_degraded`（`run_tool_loop.rs` `emit_deferred_web_degradation_if_needed`）。**残余问题**：该场景 UI 文案仍为「联网能力已降级」，与 N17「不得用能力降级概括工具/检索失败」有张力——属文案与语义问题。 |
| TipTap `onUpdate` 过期闭包导致 digest 绑错 | **不成立，撤回**               | 安装包 `useEditor` 经 `this.options.current.onUpdate` 调最新回调（`useEditor.ts:150`），`[extensions]` 只控制实例重建，不锁回调。                                                                                                                                                                                                                                                |
| N23 撤回未实现                             | **不成立，撤回**               | N23 是不变量（「撤回 ≠ Host 循环内重规划」），不是缺功能。UI（`AiMessageList` 撤回按钮）、IPC（`assistant_session_retract`）、`retractOutgoingTurn` 链路完整。                                                                                                                                                                                                                   |
| `conclude_reasoning` 仍在目标工具面        | **不成立，撤回**               | `is_exposable_tool` 仅暴露 Dispatchable 与 `spawn_subagent`；`conclude_reasoning` 为 HarnessOnly，不在可暴露面，符合架构 §6.4 兼容识别意图。残留仅是误导性元数据 `default_enabled_without_skill: true`，应清理但不是暴露漏洞。                                                                                                                                                   |
| V05 live 完全没跑                          | **表述不准**                   | MiniMax-M3、DeepSeek-Flash 做过有限实网探针；**完整逐端点矩阵与生产入口五组未完成**。                                                                                                                                                                                                                                                                                            |

---

## 三、维持有效：数据保真（P0，建议优先处理）

| #    | 问题                                                                                                                      | 位置                                                                                                                     | 影响                                                       |
| ---- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| P0-1 | **脚注定义编辑丢失**：`footnoteDef` 可编辑，导出恒写 `originalRaw`，且无 callout 那套「编辑即失效」机制                   | `editor-pm-serialize.ts:315-322`、`FootnoteExtension.ts:73-121`                                                          | 改脚注正文→保存→重开即还原，**无警告数据丢失**             |
| P0-2 | **显式保存往返改写**：①`repairTightStrongPunctuationBoundaries` 注入空格、`\*\*` 去转义；②表格对齐 `:---:` 导出固定 `---` | `markdown.ts:192-343`（经 `editor-ingest.ts:132` 进生产）、`editor-pm-serialize.ts:110-123`、`serialize-open-note.ts:20` | 未经同意改写用户 `.md`（空白注入、字面量变语义、对齐丢失） |

边界补充（复核有效）：**自动离开时另有干净基线保护**，不能把所有离开场景混为一谈；风险集中在**显式保存触发整篇重序列化**的路径。

落地顺序建议：

1. 先写 RED 测试：「编辑脚注正文→保存→重开」「带对齐表打开→显式保存」「`\*\*`/紧贴加粗打开→显式保存」；
2. 修复 `FootnoteExtension` / `editor-pm-serialize` / 关闭 ingest repair 的落盘通道；
3. 将「自动离开干净基线」与「显式保存整篇重序列化」的差异写入 [markdown-export.md](./markdown-export.md)。

---

## 四、维持有效：功能与体验

| #        | 问题                                                                                  | 位置                                                    | 说明                                                                              |
| -------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------- |
| E1       | 差异预览加载失败或未展开时仍可批准                                                    | `AssistantRunConfirmation.tsx:63`                       | 与 L03「无法确定时交付差异」的谨慎程度不对称                                      |
| E2       | 过程轨迹合并重复 `web_search` 时可能以成功状态覆盖早先失败                            | `assistant-process.ts:199`                              | 与 M9「首个失败不被后续成功覆盖」在用户过程层冲突                                 |
| E3       | 图片索引正则不跳代码围栏                                                              | `image_ref.rs:10` vs `markdown-indexing-contract.md:37` | 代码块示例图会被索引进 `image_refs`                                               |
| E4       | 格式整理内容保持对表格/HTML/零宽字符一律 `unknown`                                    | `content_preservation.rs:22`、`confirmed_changes.rs:35` | 生产门禁不会误判为通过（正确），但含表笔记的 N04 **无法自证**；触发仍依赖请求短语 |
| E5       | `capability_degraded` 文案「联网能力已降级」                                          | `AssistantRunCapabilityDegraded.tsx:130`                | 与 N17 表达要求有张力；应改为任务级限制说明                                       |
| E6       | 双路来源 UI 不分渠道                                                                  | `AssistantCitationFooter.tsx`                           | K11「保留渠道来源和各自状态」在 UI 层未投影                                       |
| F01 残留 | 清空主服务可整表 `set([])`；客户端 `mcpSearchMappingHeal` 与服务端 normalize 双修复面 | `useAiSidecarBridge.ts`、`mcpSearchMappingHeal.ts`      | 配置写入语义不唯一，牵连 D04 五组场景                                             |
| 子任务   | `context_hint`/`max_rounds` 未消费；confidence 固定 50/75；子工具缺 `web_fetch`       | `subagent_coordinator.rs`、Q06/Q07                      | 阻断 D06 / C15                                                                    |
| 双路验收 | 完整逐端点 V05 与生产入口五组未完成                                                   | G02/G03/Q17、`V05 验证窗口`                             | 机械 V03/V04 不能替代 N07                                                         |

---

## 五、维持有效：前端维护性（不与数据 P0 混排）

| 问题                                                                                                                                | 位置                                                  |
| ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `App.impl.tsx` 上帝组件（40+ hooks、大规模 prop drilling）                                                                          | `App.impl.tsx:112-1133`                               |
| design-system 大面积偏离：`bg-[hsl(var(--status-…))]`、业务层 `amber-*`、约 130 处裸 `text-[10px/11px]`；规范自身状态点写法自相矛盾 | `managementCenterPrimitives.tsx` 等                   |
| 命令面板已退役但 `command-palette.ts` 与测试仍在                                                                                    | `command-palette.ts`                                  |
| Window API 散落直引，不经 `ipc.ts` 门面                                                                                             | `useTheme`、`window-drag`、`DesktopTitleBar` 等 8+ 处 |
| TipTap 扩展清单在 `TipTapEditor` 与 `editor-roundtrip.ts` 两处手工同步                                                              | 漂移风险                                              |
| 无 a11y 自动化 / 视觉回归 / i18n 层                                                                                                 | 测试面                                                |
| 其它：命名偏离、钩子双基地、`history.depth: 80` 截断无说明、多处 `exhaustive-deps` 关闭                                             | —                                                     |

Markdown 往返保真缺口矩阵（对齐丢失、参考式链接退化、空标题 callout、折叠 callout、多行脚注、breaks 语义四套不一致、phase5 C6 虚标等）详见当轮审计底稿；**P0 以外项归入第二批**。

---

## 六、优先级建议（经复核修正后）

**第一批（数据保真，与复核排期一致）**

1. 脚注定义编辑丢失（P0-1）
2. 显式保存 Markdown 往返改写：加粗 repair、`\*\*` 去转义、表格对齐（P0-2，同热路径一起修）

**第二批（表达与合同）**

3. N17 文案回正（E5）
4. 差异预览失败时禁止静默批准（E1）
5. 过程轨迹禁止合并失败状态（E2）
6. Inline AI 边界合同澄清 + 并行触达负例（先文档/决定登记，代码改造待合同明确）
7. 图片索引跳过围栏（E3）；F01 写语义收清

**第三批（验收与底座）**

8. D04：V05 授权后跑生产入口五组；来源 UI 分渠道（E6）
9. D05：N04 可判定子集或对 `unknown` 强制差异交付；触发改为 Intake 显式操作类型
10. Q06/Q07 子任务参数消费与 `web_fetch`（D06 前置）
11. Q13 预算存档兼容分支（改任何预算默认值之前）
12. 前端维护性：上帝组件拆分、token 违规 lint、扩展清单单一来源等

---

## 七、相关文档

- [ARCHITECTURE.md](../ARCHITECTURE.md)：当前实现边界
- [agent-architecture.md](./agent-architecture.md)：目标架构
- [agent-harness/](../agent-harness/README.md)：M/C/T/N/Q/G/D 对象体系
- [markdown-export.md](./markdown-export.md)、[markdown-indexing-contract.md](./markdown-indexing-contract.md)
- [产品愿景](./product-vision.md)：产品形态原则（与本报告解耦，不改变排期）
