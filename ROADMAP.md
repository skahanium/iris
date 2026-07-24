# Iris 路线图

Iris 采用里程碑式版本规划。当前开发基线为 **v1.2.15**；本文件是版本排期的唯一来源。`ARCHITECTURE.md` 只描述已存在的结构，`CHANGELOG.md` 只记录已交付的变化。

## 产品边界

Iris 是桌面端、单用户、本地优先的 Markdown 笔记应用。长期不做：通用插件 API 或插件市场、移动端、实时多人协作/CRDT、Vault 目录级加密、浏览器扩展，以及执行任意外部代码的扩展机制。

Skills 是用户确认后启用的 prompt-only `SKILL.md` 行为包，不是安装平台，也不提供 MCP、资源、工作区、脚本或依赖安装能力。

## v1.2.16 — UI 气质升级（Wave 1 已交付，Wave 2 待办）

冷灰壳层 + 知识绿品牌点的受控刷新；不恢复纸墨/紫渐变，不换编辑器栈。分两波交付，不阻塞 v1.2.15 过程流验收。规范见 [docs/design-system.md](./docs/design-system.md)。

### Wave 1 — 管道与真相（已交付）

**Segment 1 验收（Home / 品牌轨 / 空主面）：** 冷启动有笔记时自动打开；关光 Tab 显示 WorkspaceEmpty 且不自动打开；库空显示 VaultEmpty（`workspace-empty` vault 模式）；Iris 品牌轨纯标识、不可点击；打开失败在空主面展示可读错误。人工清单见 [iris-rail-refresh-manual-checklist](./docs/testing/iris-rail-refresh-manual-checklist.md)。

**Segment 2 验收（Agent 气泡 / Composer / 过程文案）：** 用户与助手气泡轻分层可辨；发送与主操作 CTA 使用 `variant="brand"`；过程区在最终正文开始流式输出后折叠，完成摘要为「答复完毕」；历史轮次可重新展开安全过程。人工清单见 [iris-rail-refresh-manual-checklist](./docs/testing/iris-rail-refresh-manual-checklist.md)。

**Segment 3 验收（正文节奏与对比）：** `--prose-measure` 与编辑态 `text-align: justify` 保持硬锁；标题阶梯与块距消费 prose token；亮色 code/callout 对比抬升；编辑区与会话 Markdown 共用 `markdown-prose.css` 审美（无独立「导出 HTML」产品面）。

**Segment 4 验收（壳层收敛 + 文档）：** 顶栏、底栏、AI 侧车外层分隔与 Overlay 顶栏统一消费 `border-border-subtle` 与 chrome 字号阶梯（`text-caption`/`text-micro`）；Rail Tab 激活与 Outline marker 对齐 `--brand`，不改 ghost 几何。

Wave 1 另已覆盖：语义 token（`--brand`、边框三级、warning/success 表面、chrome 字号阶梯、亮色 `--status-*`）；组件消费 `shadow-overlay`/`shadow-floating`；AI activity → Composer/StatusBar；空主面/Skills/会话文案修正；Noto Sans SC、wiki（brand）≠ 外链（primary）。Overlay/搜索/管理中心入口保留在 TitleBar、快捷键与 `AppOverlays`，不经空主面 workspace 透传。

### Wave 2 — 动效、可访问性与收尾抛光（待办）

- Example callout 语义浅底收官（note/tip/warning/danger 已对齐；example 仍偏通用 muted）。
- 真实浮层 enter/exit 动效（挂 `--motion-*`）；统一焦点环；过程指示尊重 `prefers-reduced-motion`。
- AI 冷加载 skeleton；Tooltip 原语；Overlay/搜索请求 AbortController。
- 人工清单与「命令面板已退役」事实同步；Segment 4 壳层亮/暗抽检结案（见 [iris-rail-refresh-manual-checklist](./docs/testing/iris-rail-refresh-manual-checklist.md)）。

## v1.2.15 — macOS 更新与状态继承（进行中）

- macOS 已安装的 `Iris.app` 将运行时状态、缓存、临时目录、Skills 与更新缓存置于 Tauri Application Support 目录，不再写入应用包内部，确保更新安装临时目录不妨碍替换 `.app`。
- 从 v1.2.12/v1.2.13 首次升级到本版本必须按手工清单进行安全迁移：退出应用、备份旧目标目录、完整复制旧运行时目录后再通过 DMG 替换应用；不得承诺旧二进制可以完成应用内升级。
- 迁移后的 LLM/MCP 配置与加密凭据状态、人格设置、Vault 选择、Agent 会话及应用状态应保留；后续版本可恢复 macOS 应用内更新。Markdown vault 不迁移、不修改，始终保留为用户的权威文件。
- 统一 Agent Run 将在 assistant 气泡内提供可恢复的安全过程流：阶段、工具生命周期与 provider 明示 reasoning summary 在最终正文开始流式输出时折叠，普通会话历史可重新查看；不展示或保存原始思维链、工具参数与原始输出。

## v1.2.13 — 科学按需联网与韧性降级

- 联网开关表示授权；Run Envelope 使用 `offline`、`web_preferred`、`web_required` 三级语义，并记录稳定原因码。
- 本机事实、转换任务和对话元问题直接回答；模糊问题由同一回答模型决定是否调用 `web_search`。
- 单 provider 的搜索与抓取共享 10 秒预算，瞬态失败最多重试一次；失败产生非终态 `capability_degraded` 事件并继续受约束答复。
- 正常会话注入最近 6 条历史、ConversationMemory、PromptProfile、可信本机时间与上一轮脱敏安全摘要。
- 前端将能力降级显示为对话内轻量状态，红色错误仅用于整轮无法回答的终态故障。
- 普通域本地引用使用结构化轮次输入：`@` 文件以磁盘一致哈希作为单轮全文引用，`@` 文件夹与 `#` 标签仅限定本地检索范围；输入与历史气泡只显示带位置注解的浅绿色名称。

### 文档持久化与嵌入韧性

- 文档采用单标题模型：编辑器顶部标题就是 `.md` 文件名（不含扩展名），不再读写 `frontmatter.title`。旧笔记中的该字段在首次成功保存时移除；空标题不提交重命名并恢复当前文件名。
- 标题失焦或 Enter 后自动进行无覆盖、串行化的文件名迁移。Markdown 落盘成功与派生索引降级必须明确区分；迁移成功后的同一提交回执同时更新 Tab、活动路径、最近笔记和文件树，迁移失败不得回滚已保存正文。
- 打开的文档以运行期稳定 session 标识 editor surface；路径变化不得卸载 TipTap、清空选择、undo 历史或重新应用正文 baseline。只有新的权威磁盘内容 generation 可以重新 ingest 编辑器。
- 文档内存修订、完整 Markdown 快照和磁盘确认收据由单一持久化协调器管理。标题、正文、AI 应用、版本恢复、自动保存、手动保存、切换、重命名、关闭和更新安装都只能通过它提交或建立持久化屏障；路径变化、编辑器重挂载和 Tab 缓存都不能自行把内容标记为已保存。
- Markdown 原子落盘是成功的第一事实：同目录唯一临时文件写入并同步后原子替换。`file_write` 的回执包含文件条目、内容哈希和索引状态；派生索引失败只能得到 `degraded` 并排队修复，绝不否定、回滚或覆盖已确认落盘的 Markdown。
- 应用关闭、关闭标签、切换库与安装更新共享同一屏障。所有 dirty 修订获得磁盘确认前不得离开；编辑器未就绪时可使用协调器持有的完整快照，若没有可信快照则必须保留窗口与编辑状态并给出“重试 / 返回编辑”，不得把空值当作成功。
- 应用更新包仅缓存于 Iris 缓存目录；中断和网络失败保留已接收字节并在同一签名工件上续传。只有完成两次签名校验的包才能安装，过期、失配或已成功安装的缓存必须清理。
- 管理中心「使用系统代理」默认开启：HTTPS 出站（应用内更新、LLM、网页抓取）跟随操作系统系统代理及 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY`（Clash、V2Ray 等开启「系统代理」即可加速 GitHub 下载）。关闭后强制直连；切换后立即重建 HTTP 客户端，无需重启应用。不提供自定义代理 URL。
- 嵌入只由后台调度器执行。`054` 迁移将旧版零进度重建变为 `legacy_ready`，中断状态带安全失败码；模型推理不持有 SQLite 连接，低优先级批次在用户输入、打开文档或出现 dirty 文档时于批次边界暂停。启动扫描和索引提交通知会先核对当前模型、维度、来源指纹及向量长度的完整覆盖，内容未变的完整 generation 保持 `ready`，不加载模型。
- 初始索引完成、全部文档无 dirty 且连续空闲 30 秒后，`legacy_ready` 仅自动尝试一次。失败不会跨启动自动重试；关键词检索与编辑继续可用，管理中心只提供手动重试。启动残留的 `running`、`paused` 或旧 `rebuilding` 在覆盖完整时恢复为 `ready` 并清除错误；仅真实缺失或失配时标记 `interrupted_restart`，保留有效批次等待手动重试。内容指纹、模型和维度匹配的有效向量保留，增量扫描同时修复未覆盖的向量。
- 本版本在取得自动化门禁和 Windows 真机闭环证据前仍为“进行中”；验收要求见 [文档持久化与嵌入验收](./docs/testing/document-persistence-embedding-acceptance.md)，不得把计划或局部测试写作已交付事实。

## v1.2.8 — RAG 检索可靠性、Agent Task Runtime 与中文质量

目标是在不破坏既有 Markdown、搜索和 AI 工作流的前提下，完成可测量的检索基线升级，并以 Agent Task Runtime 作为主 AI 架构。

- 文档与版本基线：将工作树的发布事实校准到 1.2.6，删除过期施工资料，统一安全、Skills、迁移和检索说明。
- Agent Task Runtime：以任务生命周期、checkpoint、权限预检、工具确认、deliberation/verification 状态和可恢复暂停承载长任务；TaskPlan 是助手长任务的 Markdown-first 对话流和临时 tab 交付规则。
- 检索正确性：修复 broker 的作用域与候选截断顺序；所有向量路径都有一致的降级与诊断语义；恢复真实来源片段、span/hash 引用契约。
- 中文嵌入升级：内置 BGE-small-zh-v1.5 资源，强制迁移全部派生嵌入；旧索引只作为迁移期间的兼容回退，不混用不同维度。
- Rank v2：精确法规优先、加权 RRF、受限结构化加分、MMR 去重和来源配额；为将来可选 reranker 留出接口，但本版本不打包交叉编码器。
- 元数据与图谱：frontmatter tags、aliases 的索引与 scope 约束；链接仅用作候选扩展，输出必须携带实际文本证据。
- 评测与交付：端到端 fixture、固定 v1.2.5 基线、质量/性能/安装包体积门槛和 CI 分层检查。

## 已发布基线

### v1.2.5 — 已标记发布

`v1.2.5` 是已推送的注释标签和发布工作流触发点，保留不重写。该标签的清单版本事实曾滞留在 1.2.4；v1.2.6 开发分支在本次收口中校正当前工作树的受控版本事实，不改写历史标签。

### v1.2.4 及更早

编辑器、知识网络、会话与 AI Runtime 的历史交付记录见 git 历史和 [CHANGELOG.md](./CHANGELOG.md)。不在工作树保留已失效的实施计划。
