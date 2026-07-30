# Iris 路线图

Iris 采用里程碑式版本规划。当前开发基线为 **v1.2.18**；本文件是版本排期的唯一来源。`ARCHITECTURE.md` 只描述已存在的结构，`CHANGELOG.md` 只记录已交付的变化。

## 产品边界

Iris 是桌面端、单用户、本地优先的 Markdown 笔记应用。长期不做：通用插件 API 或插件市场、移动端、实时多人协作/CRDT、Vault 目录级加密、浏览器扩展，以及执行任意外部代码的扩展机制。

Skills 是用户确认后启用的 prompt-only `SKILL.md` 行为包，不是安装平台，也不提供 MCP、资源、工作区、脚本或依赖安装能力。

## v1.2.18 — Agent 可靠性与对话一致性（进行中）

- LLM 与联网搜索均采用用户可配置的有序主备路由：每次 Run 最多筛选主服务和两个备用，能力不匹配或暂时熔断的候选直接跳过；不采用并发双发。固定模型覆盖不参与自动切换。
- LLM 仅在尚未产生可见输出、工具调用或 continuation 前，因连接、首响应超时、限流或服务端瞬态故障切换；MCP `web.search` / `web.fetch` 在同一 20 秒预算内按顺序切换，成功服务在该 Run 后续调用中优先复用。过程流仅展示安全的切换说明，不保存端点、凭据、查询或原始输出。
- Agent intake 以本地事务 Outbox 持久化用户消息、Run 与 accepted 事件；同一 `clientRequestId` 的相同请求幂等返回原 Run，不同请求安全拒绝。网络未知回包时前端以同一 ID 重放，而不制造第二条用户消息。
- Prompt 通过版本化 `PromptContractV2` 统一编译：安全与工具边界、稳定人格、Run 领域约束、已激活 Skills、会话/材料数据按固定顺序进入；网页、Skills、历史和授权材料都只能作为数据，不能覆盖身份、权限或安全规则。人格快照在 Run 接受时冻结。
- 后续模型上下文与 ConversationMemory 仅消费“已提交对话投影”：完成轮次成对纳入；失败或活跃的孤立用户轮次保留在 UI，但绝不污染下一轮；安全终态重试复用原 turn 和用户消息。
- 用户消息气泡以内容宽度收缩包裹、最长不超过消息行可用宽度的 88%；短中文输入不会逐字折行，长文本仍受安全换行规则约束。
- 回答文末来源与处理过程使用同一类折叠交互：默认仅显示来源数，展开后才显示经过本轮引用过滤的 HTTPS 标题列表与受限证据范围说明。
- 默认打开的 Agent 侧栏在 Vault 选择阶段预热独立模块；MCP 工具发现延后至首帧之后，普通会话历史的助手 Markdown 使用共享 Worker 异步渲染，避免启动与长会话恢复阻塞主线程。
- 验收包括路由、熔断、冻结快照、对话幂等与投影的确定性测试，以及双 LLM + 双 MCP live pilot；安全和 prompt 泄露项必须为零。

### 六阶段受控演进验收矩阵（不构成发布版本承诺）

下表是 Agent Run 可靠性演进的阶段性验收边界，不改变本路线图的版本排期。每个阶段均须在独立评审中提供自动化证据；在所有阶段中，未授权工具面、联网开关作为 `web.search` 唯一授权源、涉密持久化策略和 Markdown 写入确认流程均为不可回退约束。

阶段 0 基线门禁：`web_enabled` / `web.search` 是唯一授权来源；Markdown Apply 写入必须经过用户确认，并校验 plan hash 与内容 hash；持久化事件 DTO 不包含工具参数或原始输出；classified 隔离必须保持为 CEF 加密持久化边界。

| 阶段                                           | 验收范围                                                                               | 最低自动化证据                                                       |
| ---------------------------------------------- | -------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| 阶段 0：契约校准与回归基线                     | 固定 Direct、ToolLoop、严格 Web、确认写入、断流回放与涉密隔离；同步架构、IPC、路由事实 | 契约测试覆盖安全事件投影、进程级续跑边界、Web mapping 与确认写入边界 |
| 阶段 1：Durable 恢复闭环                       | 冻结计划的预期内容 hash、检查点、恢复判定与乐观并发 Resume                             | hash 分支、重复写入防护、确认消费后的恢复测试                        |
| 阶段 2：预算、事件代码与 Intake 治理           | 冻结预算、稳定 stage code、引号外确定性分类与职责拆分                                  | 预算、事件兼容与中英文约束矩阵测试                                   |
| 阶段 3：显式只读并行子代理                     | 明确授权的 depth-1 只读 ChildRun、确定性事件顺序与结构化报告                           | 授权拒绝、并发顺序、部分失败与预算测试                               |
| 阶段 4：白名单、逐 Run 授权的通用 MCP 只读工具 | 绑定/快照、外部只读 capability 与受限输出审计                                          | migration up/down、配置漂移、授权和输出净化测试                      |
| 阶段 5：可信且可解释的 Skills 激活             | 增量索引、受限检索/重排、确定性回退与激活解释                                          | 索引增量、回退、相关性和不增权评测                                   |

阶段 4 的产品边界是“管理中心审查、用户显式信任并绑定，Composer 逐 Run 授权”：MCP 的 `readOnlyHint` 只是候选前提，不是第三方实现确实无副作用的证明；工具还必须通过本地名称/Schema 审查，并由用户确认精确 provider/tool/schema 后进入白名单，Accept 再按冻结 transport/config 执行。它不是启用 provider 后自动授权；Iris 拒绝声明或 Schema 暴露写入、发送、删除、日历变更、进程或 secret 的工具，但无法独立验证已信任服务端是否忠实实现声明。联网开关仍只控制 `web.search`，不隐式授予 `external.read`。

## v1.2.17 — macOS 更新与状态继承（进行中）

- macOS 已安装的 `Iris.app` 将运行时状态、缓存、临时目录、Skills 与更新缓存置于 Tauri Application Support 目录，不再写入应用包内部，确保更新安装临时目录不妨碍替换 `.app`。
- 从 v1.2.12/v1.2.13 首次升级到本版本必须按手工清单进行安全迁移：退出应用、备份旧目标目录、完整复制旧运行时目录后再通过 DMG 替换应用；不得承诺旧二进制可以完成应用内升级。
- 迁移后的 LLM/MCP 配置与加密凭据状态、人格设置、Vault 选择、Agent 会话及应用状态应保留；后续版本可恢复 macOS 应用内更新。Markdown vault 不迁移、不修改，始终保留为用户的权威文件。
- 统一 Agent Run 将在 assistant 气泡内提供可恢复的安全过程流：阶段、工具生命周期与 provider 明示 reasoning summary 在最终正文开始流式输出时折叠，普通会话历史可重新查看；不展示或保存原始思维链、工具参数与原始输出。

## v1.2.16 — UI 气质升级（Wave 1 已交付，Wave 2 待办）

冷灰壳层 + 知识绿品牌点的受控刷新；不恢复纸墨/紫渐变，不换编辑器栈。分两波交付，不阻塞 v1.2.15 过程流验收。规范见 [docs/design-system.md](./docs/design-system.md)。

### Wave 1 — 管道与真相（已交付）

**Segment 1 验收（Home / 品牌轨 / 空主面）：** 冷启动有笔记时自动打开；关光 Tab 显示 WorkspaceEmpty（顶栏仅搜索/新建 + 最近卡片网格，无「继续写作」hero）且不自动打开；库空为 vault 模式；Iris 品牌轨纯标识、不可点击；打开失败在空主面展示可读错误。品牌色为冷调 sage（`--brand` hue ~108）。

**Segment 2 验收（Agent 气泡 / Composer / 过程文案）：** 用户与助手气泡轻分层可辨；发送与主操作 CTA 使用 `variant="brand"`；过程区在最终正文开始流式输出后折叠，完成摘要为「答复完毕」；历史轮次可重新展开安全过程。人工清单见 [iris-rail-refresh-manual-checklist](./docs/testing/iris-rail-refresh-manual-checklist.md)。

**Segment 3 验收（正文节奏与对比）：** `--prose-measure` 与编辑态 `text-align: justify` 保持硬锁；标题阶梯与块距消费 prose token；亮色 code/callout 对比抬升；编辑区与会话 Markdown 共用 `markdown-prose.css` 审美（无独立「导出 HTML」产品面）。

**Segment 4 验收（壳层收敛 + 文档）：** 顶栏、底栏、AI 侧车外层分隔与 Overlay 顶栏统一消费 `border-border-subtle` 与 chrome 字号阶梯（`text-caption`/`text-micro`）；Rail Tab 激活与 Outline marker 对齐 `--brand`，不改 ghost 几何。**代码已合入；亮/暗人工抽检仍见清单，勾完前不视为人工验收结案。**

Wave 1 另已覆盖：语义 token（`--brand`、边框三级、warning/success 表面、chrome 字号阶梯、亮色 `--status-*`）；组件消费 `shadow-overlay`/`shadow-floating`；AI activity → Composer/StatusBar；空主面/Skills/会话文案修正；Noto Sans SC、wiki（brand）≠ 外链（primary）。Overlay/搜索/管理中心入口保留在 TitleBar、快捷键与 `AppOverlays`，不经空主面 workspace 透传。顶栏「+」新建为轻量 icon 控件（非填充 brand）；填充式肯定性 CTA 以空主面新建与 Composer 发送为准。

### Wave 2 — 动效、可访问性与收尾抛光（待办）

- Example callout 语义浅底收官（note/tip/warning/danger 已对齐；example 仍偏通用 muted）。
- 真实浮层 enter/exit 动效（挂 `--motion-*`）；统一焦点环；过程指示尊重 `prefers-reduced-motion`。
- AI 冷加载 skeleton；Tooltip 原语；Overlay/搜索请求 AbortController。
- 人工清单与「命令面板已退役」事实同步；Segment 4 壳层亮/暗抽检结案（见 [iris-rail-refresh-manual-checklist](./docs/testing/iris-rail-refresh-manual-checklist.md)）。

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
- 编辑器仅对 URL 路径以 `.gif` 结尾的 `http(s)` 网络动图使用 16:9 `cover` 裁切视口和 3% 居中安全超裁切，以屏蔽 GIF 源画布的黑边和帧间闪烁；普通图片、本地媒体、PDF、视频及无后缀 CDN 动图仍按真实比例完整显示，Markdown 不写入展示策略。
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

## v1.2.16 — Agent 严格事实核验（进行中）

- 联网开启后，外部事实默认必须以本 Run 的 Web 证据完成核验；无证据、来源冲突或联网关闭时不生成事实结论。
- 证据账本增加 Run 级关联，避免会话级去重或长对话历史被误当作本轮核验；时效事实压力矩阵作为独立硬门槛维护。
- 严格事实路径固定为“先检索并打包本轮证据、再单次无工具生成”；Web 证据包使用独立容量预算，并保证模型可见内容、Run 关联与 `[Wn]` 引用来自同一份证据。

## 已发布基线

### v1.2.5 — 已标记发布

`v1.2.5` 是已推送的注释标签和发布工作流触发点，保留不重写。该标签的清单版本事实曾滞留在 1.2.4；v1.2.6 开发分支在本次收口中校正当前工作树的受控版本事实，不改写历史标签。

### v1.2.4 及更早

编辑器、知识网络、会话与 AI Runtime 的历史交付记录见 git 历史和 [CHANGELOG.md](./CHANGELOG.md)。不在工作树保留已失效的实施计划。
