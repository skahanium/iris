# AI 双域会话与涉密协作设计

日期：2026-06-27

## 摘要

Iris 的 AI 对话区需要从“当前文档软绑定 + 涉密文档整体禁用 AI”的混合状态，改为清晰的双域模型：

- 普通 AI 是显式上下文助手。切换普通文档不自动改变会话思路，不自动把当前文档正文注入模型。
- 涉密 AI 是保险库域助手。打开涉密文档且保险库解锁时自动进入涉密域，强绑定当前涉密文档，拥有与普通 AI 对齐的编辑、检索、改写、插入能力，但所有输入、输出、会话、缓存和检索都遵守保险库安全生命周期。

本设计同时修复涉密 Markdown/目录等价性、右键快捷 AI 无效、AI 消息按钮压缩正文宽度、流式输出卡顿、长文生成被 300 秒硬截断、会话区缓存生命周期不清晰等问题。目标是一次性形成完整模型，不做“先半开放、后补安全”的分期产品状态。

## 当前问题

当前实现存在几个互相牵连的问题：

- `useWorkspaceAssistantRouting` 和 `useAssistantTasks` 会把当前普通文档路径、正文或任务提示传入 AI，切换文档会改变模型上下文；但这个绑定在 UI 上不够明确。
- 涉密文档被路由层和编辑器动作层拦截：正文、选区、写作上下文、AI 插入都不能进入 AI，导致右键菜单、快捷 AI、会话插入在涉密文档上不可用。
- 现有测试明确保护“涉密内容不得进入 AI surfaces”，因此涉密 AI 不能通过小修放开，必须建立新的安全域契约。
- 涉密文档使用独立 HTML cache/prepared open 流程，但 Markdown 输入规则、目录刷新、Ghost Spine 间距在实际体验上没有与普通文档完全等价。
- AI 消息按钮分布在左右轨道，减少正文宽度，增加换行和虚拟列表测高抖动。
- 流式输出期间 Markdown 重渲染、虚拟列表重测高、滚动跟随、后端 harness 全局 300 秒 deadline 共同造成长输出不流畅和误中止。

## 目标

- 普通 AI 不再软绑定当前文档；上下文必须来自用户明确动作。
- 涉密 AI 自动进入、自动退出、强隔离、强绑定当前涉密文档。
- 涉密 AI 会话记录持久化，但必须随保险库加密，不写入普通明文 session 表。
- 涉密历史只在涉密域可见，普通 AI 历史不可见。
- 涉密 AI 支持当前文档写入、按需检索其他涉密文档、明确目标后的跨文档修改。
- 涉密文档 Markdown 输入、渲染、目录、Ghost Spine 布局与普通文档等价。
- 涉密右键菜单、斜杠命令、选区改写、扩写、缩写、插入与普通文档同位置同功能。
- AI 会话区消息按钮不覆盖正文、不压缩右侧阅读宽度。
- 流式输出、滚动、Markdown 渲染、会话区与编辑区交互都尽可能丝滑稳定。
- 长文生成只因无进展卡死而中止，不因持续输出超过固定墙钟时间而被截断。
- 日志、trace、错误、缓存、剪贴板和导出临时态不得泄露涉密内容。

## 非目标

- 不允许普通 AI 查询、显示、插入或恢复涉密会话内容。
- 不把涉密文档加入普通 SQLite 明文索引、普通向量索引、普通 session 表或普通 trace。
- 不新增远程涉密 embedding 流程；涉密检索只使用解锁期内存索引和本地词项排序。
- 不改变编辑器技术栈，不引入新编辑器或新 UI 框架。
- 不用颜色以外的大段文字解释涉密模式；视觉区分应明确但克制。

## 核心概念

### AI 域

新增 AI domain 概念：

- `normal`：普通 AI 域，使用普通会话、普通历史、普通上下文和普通缓存。
- `classified`：涉密 AI 域，仅在保险库解锁且活动文档是涉密文档时可用，使用保险库加密会话和解锁期内存缓存。

AI 域必须贯穿前端路由、右键快捷 AI、会话历史、消息渲染、上下文装配、检索、写入、导出和后端执行。

### 普通显式上下文

普通 AI 不再因为用户切换普通文档而自动更新 `notePath` 或注入正文。以下动作才会把普通文档加入上下文：

- 右键快捷 AI、斜杠命令或编辑器 AI 操作。
- 选区引用、段落引用、标题引用。
- `@` 文档或文件夹。
- 用户明确点击“添加当前文档”一类入口。

普通 AI 会话可以持续讨论一个主题；用户翻阅其他普通文档不会暗中改变模型输入。

### 涉密强绑定

当活动编辑器是 `.classified/...` 文档且保险库状态为 unlocked：

- AI 面板自动进入 `classified` 域。
- 当前涉密文档成为默认写入目标和默认读取上下文。
- 切换到另一个涉密文档时，保存当前涉密会话状态并恢复目标文档最近涉密会话。
- 点击“新对话”才为当前涉密文档创建新的涉密会话。
- 切换到普通文档、媒体 tab、artifact tab 或保险库锁定时，立即退出涉密域，清理涉密运行时缓存，并恢复普通 AI 域。

涉密 AI 默认只能修改当前涉密文档。它可以按用户请求检索其他涉密文档；修改其他涉密文档必须有明确目标，例如 `@` 目标文档、打开目标文档或补丁确认。

## 涉密会话存储

涉密 AI 会话不得写入普通 `sessions/session_messages` 明文表。实现应新增保险库加密会话存储，复用现有 `classified_io::encrypt_cef` / `decrypt_cef` 与 `VaultKey` 能力。

推荐存储位置：

```text
.classified/.iris-ai/sessions/<thread-id>.cef
.classified/.iris-ai/index.cef
```

约束：

- `.classified` 子树已被普通索引器、搜索和 watcher 排除；`.iris-ai` 仍必须作为内部目录处理，不出现在涉密文件列表中。
- 文件名不得包含明文涉密路径或标题。`thread-id` 使用 uuid；文档路径与标题只保存在加密 payload 内。
- 普通 SQLite 不保存涉密会话标题、正文、路径、摘要、证据片段、prompt 或 tool output。
- 如果确需普通 DB 记录运行状态，只能记录非敏感计数、request id、域类型、耗时、状态码，不得记录涉密路径或用户文本。

加密会话 payload 至少包含：

```ts
interface ClassifiedAiThread {
  version: 1;
  threadId: string;
  documentPath: string;
  title: string | null;
  createdAt: string;
  updatedAt: string;
  messages: ClassifiedAiMessage[];
  evidencePackets: ClassifiedEvidencePacket[];
  tokenUsage: TokenUsage | null;
}

interface ClassifiedAiMessage {
  seq: number;
  role: "user" | "assistant" | "system";
  content: string;
  contentParts?: ContentPart[];
  toolCalls?: ToolCallInfo[];
  createdAt: string;
}
```

`index.cef` 只保存加密后的 thread summary 列表。普通历史下拉不能读取它；涉密历史下拉只能在涉密域且保险库解锁时读取。

## 涉密检索与上下文

涉密检索使用解锁期内存临时索引：

- 首次涉密检索或用户明确请求涉密库检索时，读取 `.classified` Markdown 明文到内存，切成轻量片段。
- 不写普通 SQLite、普通 FTS、普通向量库或磁盘 cache。
- 保险库锁定、切换 vault、清 AI 缓存、退出涉密域时销毁索引。
- 默认上下文只含当前涉密文档；用户请求“查保险库/涉密库/其他涉密文档”或使用 `@` 涉密文档/文件夹时才扩大范围。
- 检索排序使用本地词项、标题、路径层级、当前文档邻近度、最近编辑信息等可本地计算信号。不得调用远程 embedding 生成涉密向量。

普通检索继续排除 `.classified`，普通 AI 不能看到涉密文件名、标题、路径、片段或检索命中数量。

## 涉密编辑器等价性

涉密文档编辑器必须与普通文档共享同一套正文能力：

- TipTap schema、Markdown input rules、heading/list/table/code 等扩展一致。
- 新输入 `# `、`## `、`### ` 等标题应即时转成 heading 节点。
- `EditorOutline` 应监听涉密 editor 的 `update` 和 `selectionUpdate`，实时刷新目录和当前章节。
- Ghost Spine 使用同一套 `--editor-outline-reserve`、`--editor-outline-inset`、rail width 和外层布局约束。
- 涉密 HTML cache namespace 只能隔离缓存，不得改变 live 编辑行为。
- 关闭重开后才能识别标题属于 bug，不能作为设计行为。

涉密右键菜单和斜杠命令：

- 与普通文档同位置、同动作、同快捷入口。
- 执行域切到 `classified`。
- 选区改写、扩写、缩写、翻译、总结、插入、接受/回退走同一编辑器 transaction/patch pipeline。
- 小范围选区操作可直接落到当前编辑器；跨段、全文、跨文档修改走补丁预览或接受按钮。

## AI 面板 UI

涉密域使用琥珀/金色视觉区分：

- 面板边线、输入区焦点环、选中态、历史弹层高亮、快捷 AI 菜单 AI 项使用涉密色。
- 不增加冗余文案提示，例如持续显示“涉密 AI：当前文档名”。
- 普通域保持现有视觉。
- 涉密域与普通域的输入草稿、历史下拉、上下文 chip、消息选择状态互相隔离。

消息按钮布局：

- 选择、复制、撤回统一放在气泡左侧单列操作轨。
- 右侧不再保留操作轨，正文宽度最大化。
- 气泡内部不渲染消息级按钮，避免覆盖正文和干扰文本选择。
- 选择按钮 hover/focus 可见，选中后常驻。
- 复制/撤回 hover/focus 可见。
- 选中态只使用轻量 ring，不做整块文本高亮。

文本选择：

- 消息正文局部文字选择必须保持浏览器原生体验。
- 右键菜单使用选区快照，菜单打开后不依赖 DOM 选区仍存在。
- `Cmd/Ctrl+C` 只在当前浏览器选区位于 AI 消息区内时拦截并写剪贴板。

## 流式输出与滚动

流式输出必须优先连续感：

- token 先进入 domain-local buffer。
- 前端按固定节奏、段落边界或较大 token 增量批量提交。
- 生成中使用轻量 Markdown 渲染，避免每个 token 完整重解析表格、代码块、脚注等复杂结构。
- 生成结束后执行完整 Markdown 渲染。
- 虚拟列表只重测受影响的最后消息，避免全列表重排。
- 左侧单轨按钮和选中态不能改变正文宽度，避免流式期间换行反复变化。

滚动状态机：

- 用户在底部或接近底部时，流式输出自动跟随到底。
- 用户上滑阅读旧消息时，停止强制追底，保持当前位置。
- 新内容继续渲染到底部，但不抢滚动；必要时显示低干扰的新内容提示。
- 用户手动回到底部后恢复自动跟随。

## 后端超时与长文生成

当前 `run_harness` 的 300 秒硬 deadline 会误杀持续输出的长文创作。新的超时策略必须进展感知：

- 只要持续有 token、tool event、status event、heartbeat 或其他有效进展，就不因总墙钟时间中止。
- 无进展超过 idle/stall 阈值才判定卡死。
- 用户手动停止必须仍能在约 500ms 级别中断 stalled stream。
- 非流式、工具密集、多轮 agent 仍需要总预算和最大轮次保护，但不能覆盖正常流式长输出。
- 流式 HTTP client 不应使用会在持续输出中触发的总 300 秒 timeout；应保留 per-read stall timeout 与 abort poll。

## 内容生命周期与安全

普通域与涉密域必须隔离以下内容：

- prompt 与 system context。
- 流式 token buffer。
- 最终 assistant 输出。
- tool call arguments/result。
- evidence packets。
- Markdown render cache。
- 虚拟列表测高 cache。
- 选区快照和右键菜单快照。
- pending patch、写作候选、插入候选。
- 复制/导出临时中间态。
- 检索索引和检索命中。

涉密域清理触发：

- 保险库锁定。
- 切换 vault。
- 离开涉密域。
- 关闭最后一个涉密 tab 并完成锁定。
- 手动清理 AI 缓存。
- 应用关闭。

清理时必须：

- abort 进行中的涉密请求。
- 清空前端涉密 domain store。
- 清空 Rust 内存检索索引和运行时 buffers。
- 清空涉密选区、右键、复制/导出临时态。
- 不清除已加密持久化的涉密会话，除非用户明确删除历史。

日志与 trace：

- 禁止记录涉密正文、标题、路径、摘要、prompt、tool arguments、tool results、证据片段。
- 允许记录 request id、domain、状态、耗时、token 数、错误分类、非敏感工具名。
- 错误消息给用户时不能包含涉密内容摘录。

## 接口契约

前端需要新增或收敛到以下概念：

```ts
type AiDomain = "normal" | "classified";

interface AiDomainContext {
  domain: AiDomain;
  activeNotePath: string | null;
  classified: {
    unlocked: boolean;
    activePath: string | null;
  };
}

type AiConversationRef =
  | { domain: "normal"; sessionId: number | null }
  | { domain: "classified"; threadId: string | null; documentPath: string };

interface AssistantRequestContext {
  domain: AiDomain;
  notePath: string | null;
  contextReferences: ContextReference[];
  classifiedThreadId?: string | null;
}
```

后端需要新增涉密 AI IPC，名称可按项目风格调整，但语义必须覆盖：

- `classified_ai_thread_list(documentPath?)`
- `classified_ai_thread_load(threadId)`
- `classified_ai_thread_save(thread)`
- `classified_ai_thread_delete(threadId)`
- `classified_ai_context_search(query, scope)`
- `classified_ai_cache_clear()`

这些 IPC 必须要求保险库 unlocked，且不得返回普通 AI 可见数据。

## 验收标准

- 普通 AI 切换普通文档不会改变当前会话上下文。
- 普通 AI 只有在显式引用或快捷 AI 时收到文档正文/选区。
- 打开涉密文档自动进入涉密域；切回普通文档立即退出。
- 涉密会话历史只在涉密域可见，普通历史不可见。
- 普通 SQLite 不保存涉密 AI 正文、标题、路径、摘要、证据片段。
- 涉密会话锁定后仍可在下次解锁恢复。
- 涉密检索锁定后内存索引销毁，普通检索永远不包含涉密内容。
- 涉密 Markdown 标题输入、目录 live 更新、Ghost Spine 间距与普通文档一致。
- 涉密右键快捷 AI 与普通文档同位置可用。
- AI 消息按钮左侧单轨，不压缩右侧正文宽度。
- 文本选区、右键复制/引用、`Cmd/Ctrl+C` 在 AI 消息区可靠。
- 长文持续流式输出不因 300 秒墙钟被强制截断。
- 用户上滑阅读时流式输出不抢滚动。
- 所有 lint、typecheck、frontend tests、相关 Rust tests 通过。

## 测试策略

- 前端 contract 测试覆盖 AI domain 派生、普通显式上下文、涉密自动进入/退出。
- Hook 测试覆盖普通/涉密会话状态隔离、切文档恢复最近涉密会话、清理涉密缓存。
- Rust 单测覆盖涉密加密会话存储、锁定拒绝、普通 DB 不落涉密字段。
- 检索测试覆盖内存索引创建、查询、锁定销毁、普通检索排除涉密。
- 编辑器组件测试覆盖涉密标题输入规则、outline live 更新、Ghost Spine 间距。
- 快捷 AI 测试覆盖涉密右键菜单、斜杠命令、选区改写、插入/补丁确认。
- 消息列表测试覆盖左侧单轨、不在气泡内渲染按钮、正文选择不触发消息选择。
- 流式测试覆盖轻渲染、滚动状态机、无进展超时、持续进展不截断。
- 安全测试扫描 logs/trace/session/cache，确保涉密正文、路径、标题不落普通持久层。
