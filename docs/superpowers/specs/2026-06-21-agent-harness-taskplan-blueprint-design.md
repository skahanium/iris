# Agent Harness TaskPlan 蓝图设计

日期：2026-06-21

## 摘要

Iris 的 AI agent harness 需要从“关键词场景路由 + workflow 自动生成 UI 产物”的模式，改为“每一轮消息由 `TaskPlan` 驱动”的模式。同一个会话里，用户可以先普通聊天，再进行本地笔记问答、文字创作、研究分析、文档协作、引用核查，之后再回到普通讨论。会话记忆负责保持上下文连续性，但上一轮的场景不能锁死下一轮的任务。

最重要的产品规则是：AI 对话区必须以普通 Markdown 文字流为主。对话区可以渲染 Iris 已支持的 Markdown 内容，例如段落、标题、列表、表格、引用、链接、代码块等；但不能在消息流里渲染研究卡片、过程卡片、证据矩阵、工作区打开卡片或其他 workflow 专用 UI 控件。

结构化工作产物只在确实有价值时进入临时 tab。临时 tab 是高价值工作产物，不是 workflow 执行后的默认副产品。实现时必须主动清理旧 scene/router/artifact 代码，不能为了兼容而长期保留两套系统。

## 目标

- 每一轮都正确识别当前任务意图，不把整个会话固定到某个 scene。
- 普通请求保持轻量，避免不必要的分类模型调用、研究拆解、长上下文准备和 artifact 生成。
- 把文字创作和文档协作作为一等能力处理，不再误路由到研究任务。
- 用统一的上下文引用协议连接编辑器选区、右侧 AI 对话和后端 workflow。
- 把 `search_web` 与 `fetch_web_page` 的用户可见语义收敛成一个“联网检索/网络代理”能力。
- 在替代方案落地后尽快删除低价值临时 tab、旧 scene 驱动逻辑和重复 artifact 规则。

## 非目标

- 不构建通用工作流 DAG 平台。
- 不新增用户可见的 scene 选择器。
- 不长期并行维护新旧两套 harness 系统。
- 不让临时 tab 替代可读的对话回答。
- 不允许 AI 未经确认直接修改 `.md` 正文。

## 核心决策

### 每轮生成 TaskPlan

新增任务规划层，为每一轮 assistant 请求生成一个紧凑的 `TaskPlan`。计划至少包含：

- `intent`：例如 `chat`、`ask_notes`、`creative_write`、`rewrite_selection`、`citation_check`、`research`、`organize`、`document_check`、`chapter`、`vision_chat`、`skill_management`，或项目中等价的现有 enum。
- `confidence`：`high`、`medium`、`low`。
- `context_references`：用户显式引用的文档范围、选区或其他上下文句柄。
- `retrieval_mode`：`none`、`current_reference`、`local_notes`、`scoped_notes`、`long_document`。
- `web_mode`：`disabled` 或 `brokered`。
- `model_slot`：`Fast`、`Writer`、`Reasoner`、`LongContext`、`Vision`、`AgentTools`。
- `execution_mode`：`direct_answer`、`context_answer`、`writing_candidate`、`patch_proposal`、`structured_task`、`long_task`、`clarification`。
- `output_mode`：`markdown_message`、`artifact_backed_message`、`confirmation_required`、`diagnostic`。
- `artifact_plan`：零个或多个候选临时 tab，包含 tab 类型和生成理由。
- `requires_clarification`：仅当系统需要先用一句自然语言确认任务形态时为 true。

`TaskPlan` 是模型路由、任务策略、工具能力暴露、prompt focus、artifact 生成的唯一主依据。旧 `AiScene` 只能作为旧 session、trace 和迁移兼容字段保留。

### 路由分层

路由分三层：

- Fast Path：确定性规则处理明确 UI action、图片附件、上下文引用、明显普通聊天、明显本地笔记问答、明显文字创作。这一路径不能额外调用分类模型。
- Clarify Path：当意图低置信、或可能误入高成本任务时，在普通消息流中返回一句简短确认问题。不能静默启动研究、整篇文档检查或多工具 workflow。
- Heavy Path：只有明确或已确认的深度研究、整文档检查、多来源核查、可恢复长任务，才进入 `Reasoner`、`LongContext`、`AgentTools` 或多轮执行。

当前只靠关键词的规则过于脆弱。例如“分析”“研究”“综述”在创作或文档写作语境中，常常只是写作要求，不等于用户要求启动研究 workflow。

### 模型能力槽位

模型能力槽位由 `TaskPlan` 派生，不由旧 scene 决定：

- `Fast`：普通聊天、简单笔记问答，以及确实必要的轻量分类。
- `Writer`：创作、续写、改写、风格调整、写作候选。
- `Reasoner`：确认后的深度分析、研究综合、复杂多步推理。
- `LongContext`：确认后的整文档任务。
- `Vision`：带图片的任务。
- `AgentTools`：skill 管理或工具密集型任务。

这样普通问题不会被重流程拖慢，复杂任务也能使用更合适的模型能力。

## 对话区输出契约

AI 对话消息列表只渲染普通 Markdown 文本。它可以包含轻量 Markdown 链接或简短文字引用，用来打开临时 tab；但不能嵌入 workflow 专属卡片或控件。

研究任务的默认输出是对话区中的可读 Markdown 正文。证据覆盖、来源细节、冲突、缺口、矩阵类视图都进入“证据/来源”临时 tab。

写作任务默认输出普通 Markdown 文本。只有当用户要求应用到文档、替换选区、插入选区后方，或其他明确绑定文档修改的动作时，才生成写作修改 tab 或进入确认流程。

过程信息只在它能帮助用户恢复或理解非普通状态时出现，例如预算暂停、任务失败、任务恢复、等待确认、长任务多步骤诊断。普通完成的任务不生成过程 tab。

## 临时 Tab 分类

临时 tab 只有通过价值门槛时才生成。

1. 证据 / 来源 Tab

当存在真实来源、引用、冲突、覆盖诊断、新鲜度信息或值得查看的证据缺口时生成。“证据矩阵”最多只是这个 tab 里的一个视图；当覆盖为空或只是机械推断时，不应显示矩阵。

2. 写作修改 Tab

当存在具体 patch、替换候选、插入选区后方的候选文本、diff preview、接受/拒绝决策时生成。

3. 结构化结果 Tab

用于可复用的结构化输出，例如整理建议、文档问题清单、引用核查报告、批量操作建议。

4. 任务过程 Tab

只在长任务、暂停/恢复、失败恢复、权限等待、有意义诊断时生成。禁止显示“assistant workflow output summarized by artifact metadata”这类占位内容。

## Context Reference 系统

新增 `ContextReference`，作为编辑器选区、AI 输入胶囊、workflow 上下文和模型 prompt 之间的统一协议。

它应优先表达精确选区：

- 文档路径
- 内容 hash
- 精确 UTF-8 range 或编辑器 range
- 短展示摘要
- 可选 heading / 邻域锚点
- 引用类型，例如 selection、paragraph、heading、note、artifact
- stale / invalid 校验状态

系统必须支持跨句、跨段、局部、不完整、不规则选区。除非用户明确要求扩大范围，否则不能把所有引用自动归一到整段。

编辑器交互：

- 用户选中文档范围后，可以在文档附近打开悬浮 AI 对话栏。AI 生成结果可插入到选区后方，也可替换选区。
- 用户复制或发送选区到右侧 AI 时，输入框显示轻量引用胶囊，而不是粘贴完整原文。
- 右侧 AI 的上下文引用可用于聊天、写作、研究、引用核查和整理，不是写作专属能力。

如果执行时发现文档 hash 或 range 已失效，assistant 应提示用户刷新引用，而不是静默使用过期内容。

## Network Evidence Broker

用户可见模型只有一个联网能力，由现有联网开关控制。内部用 `WebEvidenceBroker` 收敛 `search_web` 和 `fetch_web_page` 的语义。

联网关闭时，不调用任何网络工具。联网开启时，broker 可以：

- 搜索候选来源
- 在需要时抓取 HTTPS 页面正文
- URL 去重
- 来源质量排序
- 标注新鲜度
- 记录失败和降级路径
- 将结果转换为统一证据项

对话区不暴露 search/fetch 的工具差异。证据/来源 tab 可以在有用时展示哪些来源被搜索或抓取。

风险边界单独处理：下载、需要登录的页面、非 HTTPS 来源、外部写入、高风险副作用，都不属于普通网络证据操作。

## 技术债治理

这次重构必须主动减少技术债：

- `TaskPlan` 覆盖主路径后，不保留旧 scene router 作为第二套决策系统。
- Markdown-first 消息渲染落地后，不保留 workflow 专属消息卡片。
- 不维护多套规则不同的 artifact mapper。
- 不把固定“研究结果卡片”“过程详情”“证据矩阵”行为作为兼容默认值保留。
- 旧 `AiScene` 只在历史数据、trace 兼容或阶段性迁移确实需要时保留。
- 优先删除过时测试，用 `TaskPlan` 行为测试替代；不要改写测试去继续祝福旧语义。

任何临时兼容都必须在实施计划里写明删除目标。

## 验收标准

- 同一会话可以从聊天切到创作，再切到研究，再回到聊天，不发生 scene 锁定。
- 带有“分析”“研究”等词的小说续写请求，除非用户明确要求真实研究，否则不能进入 research workflow。
- 普通 assistant 消息流中不渲染研究卡片、过程卡片、证据矩阵或 workspace 卡片。
- 研究回答以 Markdown 正文出现在对话区，证据/来源细节通过价值门槛临时 tab 查看。
- 空证据或低价值证据矩阵不生成。
- 过程 tab 只在暂停、失败、可恢复、等待确认、长任务多步骤状态下出现。
- `ContextReference` 能保存精确不规则选区，并在执行前校验是否过期。
- 联网开关通过统一 broker 控制普通网络证据行为。
- Fast-path 普通聊天和明确写作不做不必要的模型分类、研究拆解或长任务准备。
- `.md` 写入仍必须走 patch / confirmation。

## 测试策略

- 为 chat、本地问答、创作续写、研究、文档检查、图片、skill 管理等任务增加 `TaskPlan` 单元测试。
- 增加包含研究类关键词的创作请求回归测试。
- 增加组件测试，确保研究和文档结果在消息列表中只渲染 Markdown 文本。
- 增加 artifact 价值门槛测试，覆盖证据/来源、写作修改、结构化结果、任务过程四类 tab。
- 增加 `ContextReference` 测试，覆盖局部选区、跨段选区、过期引用和 hash mismatch。
- 增加网络 broker 测试，覆盖联网关闭、联网开启后的搜索+正文融合、去重、失败报告、证据转换。
- 增加安全测试，确保 patch 应用和 Markdown 写入仍需用户确认。
- 增加契约测试，确保新的 task policy / model slot 主路由不依赖旧 scene 作为主要决策来源。
