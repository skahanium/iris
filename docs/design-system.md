# Iris 设计系统

> 本文定义当前 UI 的 token、组件边界和验收规则，不承担版本排期；排期见 [ROADMAP.md](../ROADMAP.md)。

## 方向与非目标

Iris 采用扁平、安静、面向长文写作的桌面界面：编辑区优先，命令与 AI 是辅助层。避免纸墨/信纸视觉、紫色渐变、聊天主屏化、第三方主题和插件换肤。

气质：**冷灰 N 壳层 + 低饱和冷调鼠尾草绿品牌点**（`--brand` / `--knowledge-accent`，hue ~108）。灰蓝仅作 chrome focus/ring；知识交互（wiki、rail 激活、overlay 选中）统一走品牌绿。

## Token 与实现位置

主题变量在 `src/styles/globals.css`；新增或调整 token 时，先更新本文档、ROADMAP 对应事项和样式源，再修改组件。

### 品牌色层级

| 角色           | Token                                       | 用途                                                                                                                                       |
| -------------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Brand          | `--brand`（= knowledge 冷调 sage 绿系）     | wiki、rail 激活、overlay 选中、tip callout；肯定性主操作经 `Button variant="brand"`（发送等）或空主面 `variant="brandOutline"`（新建笔记） |
| Primary / Ring | `--primary`、`--ring`                       | chrome 焦点环、外链、通用控件 focus；非品牌点缀                                                                                            |
| Warning        | `--warning`、`--warning-bg`、`--warning-fg` | 非终态警示、warning callout；禁止业务层裸用 `amber-*`                                                                                      |
| Success        | `--success`、`--success-bg`、`--success-fg` | 就绪/成功徽章；禁止业务层裸用 `emerald-*`                                                                                                  |
| Destructive    | `--destructive`                             | 终态错误与危险操作                                                                                                                         |

### 表面与边框

| Token 组                                            | 用途                                                   |
| --------------------------------------------------- | ------------------------------------------------------ |
| `--background`、`--foreground`、`--panel`、`--card` | 基础画布与面板                                         |
| `--surface-chrome` / `elevated` / `inset`           | 壳层三级表面                                           |
| `--border-subtle` / `--border` / `--border-strong`  | 边框强度三级（替代随意 `/40`–`/90`）                   |
| `--shadow-overlay`、`--shadow-floating`             | 仅浮层与悬浮工具；控件禁止默认 `shadow-sm`/`shadow-md` |

### Rail 与知识

`--brand` 与 `--knowledge-accent` 同值；`--iris-rail-*` 与 `--outline-rail-active` 对齐 brand。Tab / 品牌轨 / Outline 激活态只消费 `rail.*` 或 outline 映射，不另写任意色。

### 状态与 AI

| Token 组                | 用途                                                                                 |
| ----------------------- | ------------------------------------------------------------------------------------ |
| `--status-*`            | 底栏连通性；组件用 `bg-status-*` / `text-status-*`，禁止 `bg-[hsl(var(--status-…))]` |
| `--command-highlight-*` | 命令列表焦点与选中                                                                   |
| `--ai-*`                | AI 消息、输入、流式、mention、citation                                               |

### 人格头像与 Agent 头部

- 人格头像固定为 8 个内置灰阶几何印记：`iris`、`orbit`、`axis`、`frame`、`lens`、`grid`、`flow`、`signal`。所有图形消费 Iris 的圆角方框、细线与斜切结构语言，默认 `iris`。
- 不使用 emoji、插画、上传头像、称呼首字、渐变或彩色 logo 填充。品牌绿只用于设置页已选印记的细边与弱 tint；头像本体保持冷灰。
- Agent 侧栏头部不显示空闲状态徽章、心电图或联网图标；运行与故障状态继续由过程区、Composer 和 StatusBar 呈现，不以无交互按钮重复表达。

### 字号阶梯（Chrome）

| Token            | 约值 | 用途                 |
| ---------------- | ---- | -------------------- |
| `--text-micro`   | 10px | 极少用的次级标注     |
| `--text-caption` | 11px | 底栏、徽章、辅助说明 |
| `--text-ui`      | 13px | 菜单项、列表次行     |
| `--text-body`    | 14px | 表单与面板正文       |

动效通常为 150–200ms（`--motion-fast` / `--motion-base` / `--motion-exit`）；`prefers-reduced-motion` 下必须降级。浮层进场仅 opacity（见正文规则）；文件树抽屉打开/关闭均使用透明度动效，关闭后才卸载，禁止通过位移或宽度动画推动正文；上下分层导航的树与文件列表各自使用受限视口和可见的细滚动条，禁止由内容撑高并裁出抽屉；禁止依赖未实现的 `animate-in` 空类名。

## Typography（正文）

实现：`src/styles/markdown-prose.css` + 本地字体 `src/assets/fonts/`。

| 角色        | Token / 字体                                  | 说明                      |
| ----------- | --------------------------------------------- | ------------------------- |
| Chrome / UI | `--font-sans` · Inter                         | 标题栏、按钮、设置        |
| 正文        | `--font-prose` · **Noto Sans SC**（本地打包） | 编辑器与 AI 对话 Markdown |
| 文档标题    | `--font-title` · Inter（lining nums）         | 与 chrome 同族，数字稳定  |
| 等宽        | `--font-mono` · JetBrains Mono                | 代码                      |

正文规则：

- 编辑态保持 `text-align: justify`（`inter-character`）；标题仍左对齐
- 行宽默认 `--prose-measure: 52rem`（canvas 必须消费该 token；勿擅自收窄）
- 外链 `--prose-link`（primary + 实线下划线）；wiki `--prose-wiki`（brand + 虚线下划线）
- Callout：`tip` → brand；`warning` → `--warning`；`danger` → destructive
- 亮/暗主题均需保证 code / callout / blockquote 可辨对比度
- 浮层进场仅允许 opacity 动效；禁止对居中浮层动画 `transform`（会覆盖 `-translate-*` 导致闪到角落）

## 组件边界

- `components/ui/`：shadcn/ui 基础原语与共享无业务组件（含 `Tooltip`、`SurfaceCard`）。
- `components/editor/`：TipTap、编辑器命令、查找、媒体和 Markdown 往返体验。
- `components/ai/`：助手、工具确认、消息与写作提案。
- `components/layout/`：窗口 Chrome、Rail、标题栏、Overlay 和全局布局。

可复用控件应优先使用现有 `OverlayChrome`、`IrisSurfaceMenu`、`CommandListOption`、`Kbd`、`AiComposer`、`AiMessageBubble`、`SurfaceCard`、`Tooltip`、`WorkspaceEmpty` 等原语，不能在业务组件重复实现。

### 编辑器媒体

普通图片与本地媒体以真实比例完整显示。仅对 `http(s)` URL 路径以 `.gif` 结尾的网络动图，编辑器使用稳定的 16:9 裁切视口、`object-fit: cover` 和 3% 居中安全超裁切，避免源 GIF 画布或帧间尺寸差异露出并闪烁两侧黑边；不修改 Markdown 或源文件。没有 `.gif` 后缀的 CDN 动图和动画 WebP 不做推测性裁切，仍按普通图片处理。

空主面：无打开文档时渲染 `WorkspaceEmpty`（顶栏仅右侧搜索 + `brandOutline` 新建；`workspace` 最近笔记卡片：标题 + `fileRead` 派生正文预览 `line-clamp-2` + 相对时间；`vault` 仅 muted「还没有笔记」）。禁止恢复四按钮欢迎工作台与「继续写作」hero 标题。空主面新建用 `variant="brandOutline"`；其余肯定性填充按钮（如发送）用 `variant="brand"`，勿散落 `bg-[hsl(var(--brand))]`。

全库搜索 Overlay：检索模式（关键词 | 智能）为左侧低调分段，选中用弱 brand tint，禁止 `variant="default"` 灰蓝实心 pill；执行「搜索」在右侧用 `brandOutline`。加载态用 `aria-busy` 与文案「搜索中…」，禁止 `disabled`+opacity 闪烁。「智能」即语义向量检索（`searchSemantic`）。

AI 气泡轻分层：助手近透明弱边；用户 `--ai-user-bg` 为极浅 brand tint；处理过程与来源区共用同一套紧凑的消息元信息折叠栏骨架（相同图标、文字层级、左右内边距与弱分隔线），仅以顶部/底部位置区分执行过程和证据披露；折叠摘要末项在 Run `completed` 后为「答复完毕」。同一回答的重复联网搜索在过程栏合并成单项并累计耗时，避免把同一能力的多次调用展示为冗余步骤。用户气泡以内容 `fit-content` 收缩包裹，并保留对消息行可用宽度 88% 的上限；短中文消息不得因任意断词规则而逐字折行，长文本、URL 与代码仍可在上限内安全换行。默认可见的 AI 侧栏必须在 Vault 选择屏期间预热其独立模块；首帧只初始化可交互壳层，MCP binding discovery 等非必要信息在首帧之后再请求。历史助手 Markdown 必须由共享 Worker 异步生成，等待期间显示轻量占位，不能在主线程逐条解析或高亮。

Agent 提交状态必须区分本地与远端事实：`已保存`、`正在连接模型`、`模型响应中`、`已完成`，以及失败后的「未完成，未纳入后续上下文」。只有 SQLite intake 回执成功才能显示“已保存”；它不得暗示 LLM 已收到请求。未知提交结果的重放沿用同一请求 ID，界面不重复插入用户气泡。安全终态失败的最新轮可显示“重试”，旧轮在已有后续对话时只显示“已跳过”。

### 编辑器选区与 Agent 临时关联

- 文档与 Agent 默认完全解耦；编辑器当前文档、活动 Tab 和未选中的正文都不是 Agent 的隐式输入。
- 普通文档出现非空文字选区且 Agent 可见时，Composer 内部立即显示紧凑的「当前选区」上下文条：使用 `surface-inset` 弱底色、2px `--brand` 前缘、引用图标和 ghost 移除按钮，不使用第二输入框式整圈描边。候选只在当前非空选区存在期间有效：选区折叠、取消或切换文档时立即解除，不积累历史选区，也不把选区写入会话、数据库或日志。
- Agent 隐藏时不发送也不后台保留候选；重新显示时只依据仍存在的当前非空选区重新计算。新选区替换旧选区，候选默认随下一条显式发送的消息提交，发送成功后清除。
- 候选卡片可显示前端内存中的截断预览和移除按钮；预览不进入 IPC。真正发送仍必须使用已校验的文件路径、内容哈希和 UTF-8 范围，沿用 `ContextReference` 安全契约。
- 锁定的普通文档仍允许复制和选区引用；未保存或无法映射到已提交 Markdown 的选区显示“保存后可引用”并阻止发送。涉密文档不通过此候选通道，继续使用现有 classified Agent 流程。
- 回车发送时以 ready 候选本身作为唯一引用来源并冻结本轮 `ContextReference`；只有 Run 接受后才消费候选。用户消息下方显示只读的一行选区引用预览，复制/导出不包含该元信息。历史会话只从已持久化引用投影“已附带选区 · 文件名”，不得持久化选区正文。
- 普通域 Composer 使用 TipTap 原子 `assistantMention` 节点承载 `@` 文档/文件夹与 `#` 标签；候选查询统一做 NFKC、Unicode 空白、大小写和斜杠归一化，允许多词标题、中文括号与 IME 组合输入。引用节点不可拆分，Backspace/Delete 整体移除；复制、纯文本粘贴和手工输入 `@名称` 均不会隐式恢复授权，必须重新从候选中选择。发送前再投影为自然纯文本与精确 UTF-16 `DisplayMention.range`。

### Web 引用（联网来源）

- **行内**：仅显示数字上标徽章（`sup.ai-citation-wrap` + `a.ai-citation`），字号 `--text-caption`，浅底、`--ai-citation` 前景，**无下划线**；与正文 `--prose-link` 外链区分。模型仍可输出 `[N]` / `[citation:N]`，渲染层统一为徽章。
- **文末**：助手消息正文下方固定可折叠 **「来源」** 区块（`AssistantCitationFooter`）。精确绑定时默认收起并显示自然来源类别计数，例如“用户输入 1 · 授权材料 2 · 网页 2 · 推断 1”；展开后才列出本次消息 `citation_map` 中、且在正文被精确引用的 HTTPS 来源：`序号 · 标题`（可点击打开系统浏览器）。未校准路由的来源组标题固定为 **「本次检索来源」**，展开说明仅表示该 Run 的检索范围、**不表示已逐段核验**。受控来源区是所有回答的唯一来源清单：正文不得额外生成“资料来源 / 参考来源 / Sources / References”附录、原始 URL 清单或“来源见下方”。无可验证证据时不显示模型拼接的链接。所有流式与终局使用同一净化结果：疑似尾部来源标题先暂存，确认是列表才剥离，正常正文则原样释放；不得先显示伪精确 `[Wn]` 标记，也要把模型写出的“用户提供”归因或“本轮 / 上一轮”生命周期措辞改为自然中性表达，不得在成功终局时整段回放。不展示 snippet、搜索词、工具参数、原始摘录或内部推理。
- **正文表达**：来源和核验边界由来源区承载；普通回答自然陈述事实、分析与不确定性，不展示 `Run`、`current_run_web`、`[Wn]`、来源组协议、或“本轮 / 上一轮已核验”分类。用户直接 `@` 选择的材料在来源摘要中归为“授权材料”，自动取得的本地笔记归为“本地检索”；两者都不是用户原话。仅当用户明确询问来源、核验过程或不确定性时，才用自然语言说明“我暂未查到可靠来源”等限制，仍不得泄露内部协议。
- **可访问性**：行内 `aria-label="引用来源 N"`；来源链接 `rel="noopener noreferrer"`。若模型在文末重复手写来源列表，所有回答在其进入可见正文前剥离该附录，底部受控来源区保持唯一且可访问。

## 管理中心子页与高级折叠

- LLM / MCP 供应商配置采用三级导航：AI 子页列表 → 供应商卡片 → 详情子页；详情顶栏使用 `ChevronLeft` 返回上一级，不引入 URL 路由。
- 详情页默认只展示连接凭据与核心操作（LLM 模型列表、MCP 预设与 API Key）；端点、映射、凭据引用等放入「高级设置」`Collapsible`，默认收起，分隔线使用 `border-border-subtle`。
- 进入供应商详情时同步更新 overlay 的 `managementCenterProviderId`，以支持深链恢复与面板内导航一致。
- 供应商列表行使用 `rounded-lg border-border/65 bg-background/55` 的整行可点区域；钻取入口用右侧 `ChevronRight`，勿用裸「配置」文案。MCP 联网候选直接在行内常显「主服务／备用 1／备用 2」标签；上移、下移是详情箭头前的纯图标控件，鼠标悬停或键盘聚焦时显示、粗指针环境保持可见，必须有 tooltip 与 `aria-label`，且不得嵌套在行点击按钮内。状态点：`bg-success`（就绪/Key 已配置/映射完整）、`bg-amber-500`（待完善）、`bg-muted-foreground/60`（未启用）；须配 `aria-label`。
- AI 子页标题（如「模型与供应商」「联网与证据」）**仅**出现在 [ManagementCenterPanel](src/components/settings/ManagementCenterPanel.tsx) 顶栏；进入供应商详情时顶栏标题改为供应商名、返回回到列表，子组件不得再嵌套同名返回按钮或重复 H3。
- 二级/三级详情页顶栏采用「左侧弱化返回按钮（`rounded-full` + `border-border-subtle` + `text-muted-foreground` + `aria-label="返回 X"`）+ 跨行居中标题/副标题」结构；返回按钮与主标题视觉分层，不再左对齐混排。
- 进入 MCP 第三级（`managementCenterProviderId` 非空）时，二级「联网搜索」PanelSection（当前搜索提供方、联网已开启）整体隐藏，仅保留 `McpProfilesPanel` 详情；返回列表时恢复。
- MCP 详情页的「外部只读工具」只承担 discovery、只读审查、显式信任 binding 与诊断：候选必须显示为“服务端声明只读、待用户审核”，副作用或不支持的 Schema 只汇总拒绝数量，不展示服务端原始 description。绑定操作必须二次确认精确 provider/tool/schema，并明确说明服务端 `readOnlyHint` 不是 Iris 对第三方行为的证明；取消确认不得调用 Upsert。启用 provider 或保存 binding 不等于授权；Composer 以 Run-local chip 单独勾选已审核 binding，发送后清空本次选择。classified 与 local-only 状态不得显示或提交这些 grant。
- LLM 与联网搜索路由分别显示有序“主服务、备用 1、备用 2”，使用无障碍按钮上移/下移配置；MCP 联网搜索不得再渲染独立主备卡片，服务商列表即为唯一排序入口。不得把健康度排序伪装成用户顺序。输入框显式指定固定模型时，在该轮旁说明“不自动切换”。

## 交互规则

- 主路径必须有可见入口或快捷键；纯 icon 控件必须有可访问名称和 tooltip。
- `/` 菜单仅承载文档级命令；编辑器右键菜单仅保留剪切、复制、粘贴、全选（锁定/只读时仅复制、全选），不再提供任何“AI · 选区”动作。选区与 Agent 的交互只通过助手面板中的可见临时候选完成；命令面板 UI 已退役，全局任务入口走 Overlay Family / 管理中心。
- AI 写入必须显示目标笔记、范围与风险并要求确认；不得展示或复制原始模型思维链。
- 统一 Run 的过程区属于 assistant 消息气泡而非正文：仅显示安全阶段、脱敏的工具生命周期和 provider 明确提供的 reasoning summary。生成期间默认展开；首个最终正文增量到达后自动折叠一次，之后完全尊重用户手动开关。历史会话默认折叠但可重新展开；复制、插入和导出永不包含过程区。
- 过程区不得显示工具参数、搜索词、URL、笔记路径、工具原始输出、provider 内部对象或原始 reasoning channel。过程项使用受限高度滚动，完整展示已持久化的安全事件，并应使用可访问的展开控件。
- AI 活动状态须投影到 Composer 和/或 StatusBar；禁止只写不读的 activity hint。
- 普通域 `@` 文件/文件夹与 `#` 标签在输入框和用户消息中只以内联名称呈现，使用 `--ai-mention` 浅绿色前景色；输入中的节点可使用轻量类型图标，但不得显示 `@`、方括号、胶囊或额外“引用”行。真实相对路径与类型仅用于安全 tooltip；涉密域不创建或恢复 mention 节点。
- 标题栏、Rail 和 Tab 溢出应维持当前平台窗口行为；鼠标点击顶部栏控件不得触发焦点光晕，键盘聚焦仍保留可见焦点提示；人工验收见 `docs/testing/`。
- 顶栏底色与编辑区同源（`bg-background`），不使用 `surface-chrome`，避免与编辑区形成灰带；活动 Tab 用 inset rim light（顶/左高光）+ 底部内阴影呈现玻璃质感，inactive Tab 保持透明。Tab 固定宽度 `9rem`（溢出压缩至 `4.5rem`），不随标题长度变化。
- 编辑器在中文上下文自动把 ASCII 标点转为全角（`.` `,` `:` `;` `!` `?` `(` `)` → `。` `，` `：` `；` `！` `？` `（` `）`；`"` `'` 按当前文本块内未配对计数转为 `“”` `‘’`）。仅当紧邻前一个字符属于 CJK 上下文（Han/Hiragana/Katakana/Hangul/全角符号）时转换，保护 `1.` 有序列表、URL、英文段落与 markdown 触发符；codeBlock 与 inline code 不转换。默认开启，管理中心「笔记 → 保存策略」可关。

## Iris Rail 完整刷新设计

Iris Rail 由 Rail Segments Tab、Outline Rail、AI Conversation Workspace 与 Overlay Family 组成。桌面顶栏（含无库/加载 splash 与文档态）不展示固定 Iris 品牌块，仅保留 traffic 安全区、Tab rail 与窗口拖曳区。Rail Segments Tab 只承载已打开工作区对象；Outline Rail 负责当前文档结构；AI Conversation Workspace 保持写作上下文、证据和工具确认；Overlay Family 负责搜索、图谱、设置、版本和管理中心等临时任务。

TaskPlan 体验遵循 Markdown-first：助手对话先形成可读 Markdown 草稿；临时 tab 是高价值产物，用于承载结构化结果。过程 tab 只用于长任务进度，不替代最终笔记；引用显示短摘要、来源和可追溯证据，不展示原始敏感载荷。

### 自适应工作区壳层

v1.2.19 在现有 Rail 体系中增加 Workspace Navigator 与 Agent Focus Surface。完整状态机、宽度预算和文件操作边界见 [自适应工作区规范](./adaptive-workspace.md)。

- **文档优先**：默认主平面始终是 Markdown editor surface；`--prose-measure: 52rem` 同时是宽屏布局的文档保护宽度。辅助面板不得通过继续缩窄正文来维持常驻。
- **文件入口**：标题栏 traffic safe area 之后、Tab rail 之前放置一个轻量文件树 icon 控件，具有 `aria-label="打开笔记库导航"` 与 tooltip。它不占用编辑器横向空间，且在无打开 Tab 的 workspace empty 状态仍可用。
- **浮动导航**：默认导航器从工作区左侧以非模态抽屉出现，宽度 `18rem`，窄窗口为 `min(18rem, calc(100% - 3rem))`。抽屉消费 `bg-panel`、`border-border-subtle` 与 `shadow-overlay`，不得做成新的管理中心卡片。
- **固定导航**：只有布局预算满足时才显示可用的固定操作；固定态参与宽度分配，失去预算后自动退回浮动而不关闭。用户的固定偏好可以持久化，临时开关状态不持久化。
- **导航器视觉**：标题行仅使用“笔记库”作为简洁分区名称；不在此处重复展示图钉或快捷键提示。标题下方是两个紧凑、无业务卡片感的 icon 工具条：文件夹层提供当前目录新建文件夹、排序、全部展开/折叠；文件层提供直属媒体显示、当前目录搜索、排序和新建笔记。所有 icon-only 控件都有中文可访问名称、tooltip 与可见焦点；不加入拖放、批量操作或永久三栏。
- **上下分层语义**：上层是纯文件夹 tree，含根目录节点与直属 Markdown 数；点击名称只选择下层范围，箭头或左右键才展开/收起。下层是纯直属文件 list，默认只显示 Markdown，眼睛开关才加入图片、PDF、视频；搜索只过滤当前目录可见范围，切换目录、清空或 Esc 后收起。文件夹和文件分别提供独立的名称/数量、名称/更新时间排序。
- **目录树层级与动效**：文件夹以展开/收起图标表达状态，嵌套项目沿真实祖先分支绘制细的冷灰连续导轨；文件行使用文档或媒体图标并保留锁定状态。选中目录与当前文件仅使用弱 brand tint、brand 前缘 marker 与文字，不使用厚边框或分类色。箭头、hover、选中态和下层切换只使用 150ms token 动效；下层只做 opacity，不位移；`prefers-reduced-motion` 下立即切换。
- **分层尺寸**：默认文件夹/文件区为 `45/55`，中间水平分隔线可指针拖动、键盘方向键按 5% 调整、双击恢复；上层限制 `25%–70%`。该比例、两层排序与媒体显示偏好可写入不含路径的 localStorage；选中目录与搜索词不得持久化。
- **Agent 侧车**：侧车目标宽度为 `30rem`，允许在 `25rem–45rem` 间调整。拖动上限必须同时受文档保护宽度约束；空间不足时显示可见 Agent 入口，打开后进入主区阅读，而不是把正文压到保护线以下。
- **Agent 主区阅读**：使用同一 `UnifiedAssistantPanel` 实例，不创建 Agent Tab。主区消息、过程流、确认面与 Composer 居中限制为 `--ai-focus-measure`（`70rem`）；点击“返回文档”或任一文档 Tab 退出，编辑器不得卸载或重建。
- **响应式稳定性**：窗口 resize 可以改变辅助面板的有效 presentation，但不得自动切换当前文档、创建 Tab、取消 Run、清空 Composer 或改写 Markdown。禅模式暂时隐藏导航与 Agent，退出后恢复进入禅模式前的用户意图。
- **品牌与层级**：导航器当前文件沿用 brand marker；文件夹展开、hover 与焦点使用现有 muted/accent 层级。Agent focus 仍消费 `--ai-*` 与共享 conversation prose，不引入新的聊天主屏视觉语言。
- **主区内容列**：消息流、确认区、recovery、外部工具授权边界、选中消息操作条与 Composer 统一使用 `.ai-focus-column`（`margin-inline: auto; width: 100%; max-width: var(--ai-focus-measure)`）。`--ai-focus-measure`（`70rem`）是主区阅读专用 token，与文档 `--prose-measure`（`52rem`）分离——focus 独占主区、文档隐藏，允许更宽内容列容纳代码与表格；侧车与文档仍消费 `--prose-measure`。两者都不硬编码 px。
- **焦点管理**：进入主区阅读时焦点送到消息流（`tabIndex={-1}`），不强制进入 Composer；返回文档时恢复进入前的焦点位置。`Ctrl/Cmd+\` 打开导航后焦点进入树由导航器接管，关闭后返回标题栏入口。

### Workspace Navigator 组件边界

- `components/file/` 承担目录树、文件行与文件操作业务；`components/layout/` 只承担抽屉/固定 placement 和窗口宽度协调。
- 轻量导航器使用上下分层 navigator：上层只选择/展开文件夹，下层只打开当前文件夹的直属文件。外部切换文档时自动选择并展开其父目录；目录删除时退回最近存在父目录，最终退回根目录。单击下层文件打开但不关闭抽屉；Esc、再次点击入口或把焦点明确返回编辑区时关闭浮动抽屉。
- 常用文件操作必须复用 `useVaultCatalog` / `useVaultFileActions` 共享 controller 与 `useNavigatorFileLifecycle` 屏障。禁止在轻量导航器内复制 `fileRename`、`fileDelete`、锁定或 dirty flush 的独立流程。
- 删除文案固定为“移入回收站”，继续使用确认对话框；永久删除不得出现在轻量导航器。
- 管理中心“浏览笔记库”继续承载双栏管理视图、批量操作、语料库类型和模板；`Ctrl/Cmd+Shift+E` 语义保持为完整库管理。轻量导航器使用 `Ctrl/Cmd+\\` 切换。

### 壳层边框与字号

顶栏、底栏、AI 侧车外层分隔线与 Overlay 顶栏统一使用 Tailwind `border-border-subtle`（映射 `--border-subtle`），避免在壳层散落 `border-border/60` 等任意透明度。底栏、徽章与次要标注优先 `text-caption`（`--text-caption`）或 `text-micro`（`--text-micro`），不用裸 `text-[11px]`。Rail Tab 激活态与 Outline 当前章节 marker 仅消费 `--brand`（经 `--iris-rail-active` / `--outline-rail-active`），不改变 Outline ghost 几何与留白合同。

## 验收

### 文档持久化与嵌入状态

编辑器顶部标题等于当前 `.md` 的文件名（不含扩展名），是唯一可编辑标题；它不写入 Markdown frontmatter，也不由 SQLite 标题反向覆盖。标题输入框按内容自动增高，不使用内部纵向滚动条；失焦或 Enter 后自动同步文件名。空标题不能提交，恢复现有文件名；同步失败时保持已保存内容和旧路径，并以“标题未同步到文件名，可重试”的可操作状态呈现。

文档重命名后的 Tab、最近笔记、文件树与 editor surface 必须消费同一提交回执。editor surface 以稳定 document session 而不是路径作为身份，改名或普通 catalog 刷新不得触发 TipTap 卸载、`ready(null)`、正文 baseline 重置或可见闪烁。

状态栏和 Tab 只能消费文档持久化协调器的投影，不能根据 `activePath`、编辑器是否挂载或本地 Tab 缓存推断“已保存”。对有路径的文档，状态栏必须始终显示以下其中一个状态：`正在保存`、`已保存`、`保存失败`、`已保存但索引待修复`。其中 `正在保存` 覆盖尚未收到当前修订落盘回执的 dirty/saving 状态；只有对应修订的 Markdown 磁盘写入成功才可显示 `已保存`。有路径且相对本次打开的正文发生去空白字数变更时，在「X 字」旁以低饱和绿/红展示 `+N` / `−M`（`tabular-nums`，零变更不展示）；统计来自编辑器事务累计的去空白字数（与「X 字」同源），非磁盘 Markdown diff；基线为本次打开时的正文，保存不重置。

`保存失败` 是阻断性、可操作的错误：关闭、关闭标签、切换库和安装更新必须停留在当前界面，提供清楚的“重试 / 返回编辑”路径。编辑器重挂载期间，若协调器没有可信的完整 Markdown 快照，同样必须阻断这些操作；不得将空内容、`null` 或编辑器未就绪呈现为成功。索引待修复不是保存失败：Markdown 已安全落盘时，允许关闭和更新，状态用中性色或弱警示色说明派生索引正在修复。

管理中心只读取嵌入调度器的完整状态，而不本地拼接进度。`开发环境未启用`、`旧版检索可用，等待空闲升级`、`后台重建`、`已暂停`、`失败但不影响编辑` 和 `就绪` 必须与调度器 phase 一一对应。调试构建的 `disabled` 是中性状态，说明 `npm run dev:desktop:embedding` 的显式启用路径，不显示错误或重试；正式发布包不进入该状态。运行时可暂停；中断恢复的 `paused` 文案必须说明会在空闲后续建，普通失败仅显示安全失败原因和手动重试，不展示模型原始错误、笔记标题、正文或路径。后台重建不得遮挡编辑器、阻止 Markdown 保存或伪装成全局加载态。

这些状态文字应使用 `role="status"` 与 `aria-live="polite"`，并在亮/暗主题和窄窗口下保持可辨识；红色仅用于实际保存失败，不能用于已保存但索引待修复或可恢复的嵌入失败。

应用更新的缓存进度属于可恢复状态：发现部分缓存时以中性文案说明将继续下载，不展示发布时间；签名验证或预检失败才使用错误状态，且不得丢弃可安全续传的部分工件。

「使用系统代理」位于管理中心总览的系统边界、应用更新上方：默认开启，用既有 `SwitchControl` 即时写入设置；副文案显示当前生效端点（如 `127.0.0.1:7890`），关闭或未检测到代理时显示「无代理」。不展示代理凭据。

### AI 能力降级状态

`capability_degraded` 是对话内的轻量、非终态状态：使用中性色或弱警示色，显示能力名称、用户安全说明和可重试提示，不遮挡已生成内容，也不触发全局红色错误条。只有模型完全不可用、权限拒绝、持久化失败或非法请求等整轮无法回答的故障使用红色终态错误。降级状态必须可由键盘和读屏器感知，并与最终 `completed` 状态同时成立。

主模型或搜索服务在尚未产生可见输出时切换备用，应在过程区显示「主模型/搜索服务不可用，已切换到备用」的中性可访问状态；不显示 provider endpoint、模型凭据、搜索词、URL 或原始错误。已产生可见 token、工具调用或 provider continuation 后不显示“自动切换”，而按当前 Run 的安全终态处理。

### AI 过程流

过程事件应按 Run 与 assistant 消息一一绑定，不能以全局“最近过程”覆盖历史气泡。普通会话可从既有 Run 事件恢复安全阶段、工具状态和 provider summary；classified 会话维持易失处理，不创建历史过程记录。reasoning summary 仅限 provider 的显式可展示摘要，必须经过可见文本清理、敏感信息脱敏和长度限制；任何不支持该通道的模型仅显示阶段与工具进度。

UI 改动至少验证亮/暗主题、键盘导航、窄窗口/Tab 溢出、`prefers-reduced-motion`、错误与加载态，并运行 lint、format、typecheck 与相关测试。涉及文档持久化或嵌入状态时，还必须验证上述保存/降级状态、关闭与更新阻断、暂停/继续和手动重试；涉及编辑器 schema 时，还必须运行 Markdown parse → node tree → serialize 往返测试。
