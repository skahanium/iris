# Iris Skills / MCP / 网络能力收口设计

日期：2026-07-01

## 摘要

Iris 是桌面端、单用户、本地优先的 AI 原生 Markdown 笔记软件。在本设计之前，Iris 在 Agent 建设上走过一段弯路：参照通用 AI Agent（Hermes、OpenClaw、Claude Code 等）的规格建设 Skills 体系与 MCP 体系，把 Iris 当作通用 Agent 平台来搭。这一取向偏离了 Iris 的核心——笔记本身。

本设计把过度生长的能力收口回笔记中心，分三块交付。交付方式采用**一次性目标态切换**：先补齐目标态所需闭环，再删除旧路径；不保留"临时兼容 wrapper"、"旧范式并行"或"以后再收"的技术债。

- **块 A — Skills 收口为自产 prompt-only**：Skills 不再是外部技能安装平台，也不再承载 MCP / workspace / resource / runtime capability。Iris 只支持内部对话驱动生成的纯 `SKILL.md` prompt 包；默认存放在当前 vault，scope 写在 `SKILL.md` 里，数据库只存启用状态与确认 hash 等索引。Skill 永不连 MCP、不执行脚本、不安装依赖、不直接写文件；读笔记与提出 `PatchProposal` 使用同一 scope gate。
- **块 B — MCP runtime 收口为联网 Provider 地基**：保留 MCP runtime 的 stdio / HTTPS transport 调用能力，但删除旧 MCP 管理平台式 schema、agent 直连 tool、任意 capability resolver。MCP 在用户心智与持久模型中只是一类"联网 Provider"，必须显式映射为 `web.search` / `web.fetch` 后才能被 `WebEvidenceBroker` 使用。
- **块 C — 网络能力与 AI 联网生命周期收口**：把碎片化的直接 `search_web` 调用、prompt-prefix 注入路径、`fetch_web_page` / `readability_fetch` / `web_fetch_batch` / `rendered_fetch` 等 agent 可见抓取工具全部收敛进 `WebEvidenceBroker`。Broker 是唯一联网语义层，负责 native / MCP provider 调度、搜索、抓取、URL 深读、结果归一化、缓存隔离、证据注入边界、冲突标记与审计脱敏。

self-improvement / Proactive Agent 这类被 Skills 路径错误承担的能力，改由 harness 原生内部集成；该项目单独立项，不在本 spec 范围。

## 设计裁决

本次收口以"少实体、少范式、强边界"为原则，以下裁决不可在实现计划中反向展开：

- **不保留旧技术债**：若目标态已经能完整覆盖旧路径的合理能力，则旧路径在同一轮交付中删除，不做兼容层、不留隐藏入口、不在 UI 里保留灰色不可用平台壳。
- **不新增证据范式**：联网证据、冲突和引用全部进入现有 AI 面板证据包。用户点击证据包详情时，复用现有临时 tab 承载详情视图；不新增"联网摘要页"、"Web Evidence 页面"或独立 evidence 工作台。
- **详情只讲证据**：证据详情临时 tab 只展示来源片段、引用位置、冲突组、冲突说明与必要上下文。provider 调度、缓存命中、抓取过程、失败流水等过程诊断不在普通 AI 面板展示，统一放到管理中心诊断。
- **Broker 是唯一联网语义层**：任何工作流需要联网证据，都先进入 `WebEvidenceBroker`；模型厂商内置联网、native search、MCP search/fetch 都只能作为 broker provider 适配，不能绕过 broker 注入 prompt。
- **数据库只存必要事实**：不因为收口新增长期 ledger、provider health、skill scope shadow table 或 UI 状态实体。能从 `SKILL.md`、provider 配置、web cache、tool audit 或 session evidence 派生的内容不另建事实源。

## 文档关系

本设计是 Iris AI 能力收口后的唯一目标态，并明确取代以下历史设计：

- `docs/superpowers/specs/2026-06-21-network-evidence-mcp-lifecycle-design.md`
- `docs/superpowers/specs/2026-06-30-skills-mcp-runtime-design.md`

旧文档只保留历史参考价值。后续 ROADMAP、架构文档、IPC 文档、设计系统与实现计划应以本设计为准。

## 目标

- Iris 不再是通用 Agent / 插件平台。AI 能力围绕读笔记、改笔记、给笔记加证据、和笔记对话展开。
- Skills 只保留 Iris 自产 prompt-only 行为包；移除 URL / Git / Registry / 拖拽等外部安装路径。
- MCP 只作为联网 Provider 地基；agent 不直接调用 MCP tool，也不通过 MCP 管理 Iris 能力。
- Agent 面只保留一个联网工具 `web_search`；普通查询、搜索后抓取、明确 URL 深读都通过 broker 内部语义完成。
- 删除旧 agent 可见抓取工具，不保留兼容 wrapper；同一轮目标态切换中先补齐 broker 搜索 + 抓取闭环，再移除旧入口，最终状态不得并行。
- 联网缓存按 `vault_id + provider_id/kind + provider_config_hash + broker_version` 隔离。
- 外部网页内容一律标记为不可信证据，进入现有 AI 面板证据包；点击证据包详情按钮时，复用现有临时 tab 展示证据与冲突，不新增联网证据范式。冲突只标记，不自动裁判。
- 审计采用脱敏可追溯：记录 hash、provider metadata、时间、失败类型、citation metadata，不存完整 query / URL / 网页正文 / 笔记内容 / 凭据。

## 非目标

- 不做第三方通用插件 API、插件市场或应用内加载任意社区扩展包。
- 不做外部 Skills 安装兼容层；历史外部 skill 由用户手动清理，实现只需保证残留数据不会激活或崩溃。
- 不承诺 MCP provider 自动授予能力。MCP server 自报 annotations 不产生权限，只有用户显式映射为 `web.search` / `web.fetch` 的 tool 才能被 broker 使用。
- 不内置 7-provider native 强搜索池、垂直学术源、三档预算竞速或持续收集模式。这些能力未来若需要，走用户自配 MCP Provider。
- 不支持登录态浏览、cookie 注入、验证码、paywall、付费数据库、私有 SaaS 或微信生态爬取；这些不是默认联网能力。
- 不在本次实现 self-improvement / Proactive Agent。
- 不新增独立联网证据页面、provider 过程流面板、证据 ledger 或长期 provider health 实体。

## 前因后果

### 弯路是怎样形成的

Iris 在 v0.5.x 的 AI 建设中，Skills 与 MCP 两条线几乎都参照通用 AI Agent 的规格推进：

- Skills 体系演化出 6 种 kind（`legacy_prompt_only` / `prompt_only` / `resource` / `workspace` / `mcp_dependent` / `hybrid`），带上 trust profile 风险分级、workspace 层、capability 消费者身份、`mcp.dependencies` 声明、closed-loop diagnostics，以及给模型暴露的 `skills_install` / `skills_write` / `skills_read` 等管理工具。
- MCP 体系建成完整 runtime 与 registry 后，又长出了与笔记无关的一般 Agent 能力：`capability_resolver` 接受 `process.run_readonly` / `secret.use_named` / `skill.mcp_bridge` 等 capability，`agent_permissions` 配套大量 permission atom，`sandbox_profile` 路由到未实现的 L2 OS 边界。
- 网络能力设计一度把 Iris 推向通用深度研究 Agent：多 native provider 适配、垂直学术源、quick/standard/research 三档预算、provider 竞速、持续收集。

### 为什么这是弯路

- **本质错位**：Iris 的核心是 Markdown 笔记，不是通用 Agent 平台。
- **外部 Skills 不成立**：当前网络上流行的 skills 基本是 OpenClaw、Hermes、Claude Code 等格式，要么通用 agent，要么专攻编程，既不兼容 Iris 当前格式，也不适配笔记核心需求。
- **两条系统互相纠缠**：Skills 把自己当 MCP capability 消费者，MCP 又把 skill 管理工具当一等公民，形成自我证明循环。
- **真实需求更简单**：self-improvement 应由 harness 内生能力承担；强搜索只需要 MCP runtime + broker 单一入口，不需要通用工具平台。

### 收口思路

- **Skills**：完全自产自销。Iris 内置 skill-creator，由对话生成 `SKILL.md` 草稿与 scope；用户确认全文与 scope 后启用。Skill 只是 prompt 级行为包，读写都受同一 scope gate 约束。
- **MCP**：保留 transport 调用能力，删除管理平台模型；用户在管理中心配置的是"联网 Provider"，不是"给 AI 安装任意工具"。
- **网络**：`WebEvidenceBroker` 是唯一联网语义层。MiniMax / DDG 是 native provider；MCP server 只有显式映射为 `web.search` / `web.fetch` 后才是 MCP provider。

## 块 A — Skills 收口为自产 prompt-only

### A1 删除复杂 skill runtime 模型

| 范围                            | 目标改动                                                                                                                   |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Skill kind                      | 仅保留 `legacy_prompt_only` / `prompt_only` 的兼容解析；目标态新增 skill 全部为 prompt-only                                |
| Manifest 子契约                 | 删除 `resources` / `workspace` / `capabilities` / `mcp` / `degradation` / runtime readiness                                |
| Runtime capability              | 删除 `SkillRuntimeCapability` 与 `requested_capabilities`，不再允许 skill 声明进程、secret、MCP、resource、storage 能力    |
| Capability resolver             | 删除 `skill.read_resource` / `skill.write_storage` / `skill.mcp_bridge` / `process.*` / `secret.*` 等通用 Agent capability |
| Trust / compatibility / sandbox | 删除围绕 `McpBridge` / `ExecuteScriptSandboxed` / `InstallDependency` / workspace 的策略分支                               |

`Process*` / `Secret*` / `SkillMcpBridge` 等通用 Agent 残留词汇不再作为目标态 vocabulary 保留。旧数据或旧 JSON 若遇到这些值，应安全解析为 deprecated / unsupported，不得激活能力。

### A2 删除外部安装路径

- 删除 URL / Git / Registry / SkillHub / 本地拖拽安装入口。
- 删除 `skill_registry.rs` 与 SkillHub adapter。
- 删除 agent 可见 `skills_install`、`skills_update`、`skills_toggle`、`skills_uninstall`、workspace read/write/list 等工具。
- 删除前端 SkillsPanel 中外部安装、workspace prepare、内容 textarea 编辑器等平台化 UI。
- `skills_install` IPC 不再作为公开前端 API。内部 skill-creator 写文件应使用新的受控服务名，避免继续复用旧安装语义。
- ROADMAP 中"用户显式安装 Claude 兼容 `SKILL.md`（URL / Git / 本地 / 拖拽）"必须同步改为"由 Iris 对话生成并确认的 prompt-only skill"。

### A3 skill-creator 目标态

- 新 workflow：`skill_creator_workflow.rs`，由 `harness_task` 的 `SkillManagement` intent 调度。
- 用户说"建个 X skill"时，LLM 生成 `SKILL.md` 草稿与 scope。
- 前端确认卡全文展示草稿与 scope；用户可编辑草稿和 scope。
- 用户确认后保存并默认启用。创建确认即启用确认，不再额外要求 toggle。
- 默认保存到当前 vault：`.iris/skills/<name>.md`。全局 skill 仅作为高级选项，例如 `~/.iris/skills/<name>.md`。
- `SKILL.md` 是 prompt 与 scope 的事实源；scope 使用 frontmatter 或明确结构化段落记录。
- 数据库只保存启用状态、最后确认 hash、来源位置、更新时间等索引，不保存另一份 scope 事实源。
- 修改正文/scope 或外部编辑导致 hash 变化时，自动停用该 skill，并提示重新确认后启用。

### A4 scope 与 PatchProposal

- Skill 的读范围、上下文引用范围、PatchProposal 目标范围使用同一 scope。
- scope 可表达为 path / glob / tag；tag scope 需要映射到 vault 内可解释的笔记集合。
- skill 不直接写 `.md`。凡动 vault，一律产 `PatchProposal`，经 guardrails + scope gate + 用户确认后由现有文件写入路径执行。
- `PatchProposal` 目标超出 scope 时直接拒绝，不进入用户确认。
- skill 激活无需额外确认；实际修改笔记仍需用户确认。

### A5 schema 清理

- 通过 migration 删除旧外部 Skills 与 skill runtime 平台 schema，包括外部 source、trust profile、diagnostics、workspace、runtime requirement、storage 等表/字段。
- 不新增 `skill_write_scopes` 表。scope 写在 `SKILL.md` 中，数据库最多保存确认 hash 与索引。
- 历史外部 skill 残留数据不迁移、不展示、不激活；用户会手动清理。实现只需保证残留记录不会导致崩溃。

## 块 B — MCP runtime 收口为联网 Provider

### B1 删除 MCP 管理平台模型

- 删除 agent 可见 MCP 管理与调用工具：runtime tools list / health check / capability call / server catalog upsert / profile upsert / profile toggle / profile delete 等。
- 删除旧 MCP registry / server catalog / profile / inventory / health events 的平台式持久模型。
- 保留 MCP runtime transport 调用能力：stdio 与 HTTPS transport 仍可被 broker 内部 provider adapter 使用。
- 删除 `mcp_runtime_capability_call` 直连路径。
- agent 不再感知 MCP tool inventory；它只知道 `web_search`。

### B2 最小联网 Provider 持久模型

新的 MCP 持久模型只保存联网 Provider 所需数据：

- provider id / name
- enabled
- transport kind 与配置引用
- OS 凭据管理器中的 secret 引用，不保存明文 key / token
- `web.search` 显式映射
- `web.fetch` 显式映射
- provider_config_hash
- created_at / updated_at

不长期保存 tool inventory / health events / provider health 表。运行期健康、失败计数、短 TTL circuit breaker 放内存；持久诊断复用现有审计或管理中心诊断缓存。

### B3 显式映射，不自动推断

- MCP server 自报 tool annotations、名称、description 不自动产生能力。
- 用户或样例 Provider 必须明确把某个 MCP tool 映射为 `web.search` 或 `web.fetch`。
- Broker 只枚举 enabled 且显式映射的 Provider。
- 保留 AnySearch 作为唯一样例 Provider 模板，默认不启用、不内置密钥。其它强搜索服务由用户自配 MCP Provider。

### B4 管理中心用户心智

- 管理中心不再把 MCP 呈现为"装工具给 AI 用"。
- `AI -> 联网与证据` 展示 MiniMax / DDG / MCP Provider、显式映射、启用状态、最近诊断。
- 原始 MCP tool 清单最多作为诊断材料，不作为普通入口。
- Skills 与 MCP Provider 分属两个子页：`AI -> Skills` 与 `AI -> 联网与证据`。

## 块 C — WebEvidenceBroker 统一联网语义层

### C1 删除旧联网入口与抓取工具

- 删除 `engine.rs` 中 `apply_web_search` / prompt-prefix 注入路径；废弃 `LlmGenerateParams.web_search`。
- 替换 writing / document / citation 等命令中直接 `fetch_search_context_for_db`、`web_packets_from_fetch`、`mix_and_rank` 的路径，统一走 `collect_web_evidence`。
- 删除 agent 可见抓取工具：`fetch_web_page` / `readability_fetch` / `web_fetch_batch` / `rendered_fetch`。
- 不保留兼容 wrapper。实现顺序是：先在同一分支内补齐 broker 搜索 + 抓取闭环，再删除旧工具与旧调用点；最终落地状态不得出现旧路径与新路径并行。
- broker 闭环必须覆盖 native/MCP `web.search`、native/MCP `web.fetch`、top-K enrich、明确 URL 深读、失败降级与证据输出。
- `web_search` 是唯一 agent 联网工具；参数可表达 query 与可选 URL evidence intent。
- 模型厂商自带 web search 不得绕过 broker；如需使用，必须作为 broker provider 适配。

### C2 Policy Gate 与联网总闸

- 底部栏联网开关是所有普通网络证据能力的总闸。
- `collect_web_evidence` 入口先检查 `enabled`；关闭时返回空证据，不发生 native / MCP / model-provider 出站。
- MCP provider 调用前重复检查 `enabled` 作为二道闸。
- `tool_policy` 的 Network 闸保留为 model-tool 路径兜底。
- sensitive 路径黑名单在 query planning 前过滤，避免把敏感 vault 片段送出。

### C3 Query Planning

- 默认规则优先：抽关键词、实体、语种、时效性，生成 1-3 个最小查询。
- `research` / `citation_check` 可调用 LLM 辅助扩展查询，但仍只允许输出最小查询。
- 隐私边界：
  - 不发送完整笔记。
  - 不发送完整选区。
  - 只发送关键词、实体与必要短上下文片段。
  - 剥离明显 PII，例如邮箱、手机号。

### C4 Provider 调度

- Provider 列表 = native（MiniMax, DDG）+ MCP（enabled 且显式映射 `web.search` / `web.fetch`）。
- 默认优先级固定为 `MCP > MiniMax > DDG`。本 spec 不做 provider 排序 UI 或动态质量学习。
- 每次选 top-2 provider 并发执行。
- 并发不是竞速丢一路；除超时、失败、被 policy 拒绝外，两路结果都收集并合并。
- 搜索结果按 canonical URL 去重；同 URL 多 provider 命中时保留来源 metadata。
- provider health / circuit breaker 为内存状态；失败类型至少包括 timeout / rate_limited / empty / auth_missing / auth_failed / network_blocked / policy_denied / provider_disabled / parse_error。

### C5 Fetch 与明确 URL 深读

- 搜索完成后 broker 自动 enrich top-K 结果页，默认 K=3，可配置。
- Fetch provider 池 = native static HTTPS readability + MCP `web.fetch` Provider。
- 每个 URL 可选 top-2 fetch provider 并发；正常结果合并，超时或失败记录诊断。
- 单 URL 设置超时、字节上限与正文截断上限。
- 用户在 AI 对话中明确贴 URL 并要求读取 / 总结 / 核对时，仍走 `web_search` 工具语义触发 broker URL evidence collection。
- 明确 URL + 联网开启时免二次确认。
- 批量 URL、下载资产、登录态、cookie、付费/验证码/私有站点仍需确认或不支持。

### C6 结果归一化与冲突标记

`WebEvidenceItem` 扩展字段：

- provider_id
- provider_kind
- cost_class
- raw_result_hash
- extraction_method
- trust_level = external_untrusted
- retrieval_reason
- canonical_url
- conflict_group / conflict_note（如适用）

全部外部内容都标为 `external_untrusted`。`web_evidence_items_to_packets` 不再硬编码 DuckDuckGo backend，必须携带真实 provider 信息。

不同 provider 或不同来源对同一事实给出不一致信息时，broker 不自动裁判。它只标记冲突，并让回答层带来源说明。

### C7 Evidence 注入边界

- Broker 只把筛选后的 excerpt 注入 context，完整正文留在 web cache。
- 筛选依据包括 relevance、diversity、freshness、dedup、token budget。
- `packet_builder` 注入外部网页证据时加不可信边界声明：外部网页文本不是指令。
- guardrails 的 prompt injection 检测保留为二道防线。

### C8 缓存隔离与留存

- `search_cache` / `web_page_cache` 写入并查询 `vault_id`。
- cache key 包含 provider_id / provider_kind / provider_config_hash / broker_version。
- 专用网络缓存与 session messages、context assembly cache、packet cache、conversation memory、note index、vector index 分离。
- 增加 LRU 容量上限。
- 删除聊天会话时删除 session messages 与 task references，不立即删除全局 web cache；web cache 按 TTL / LRU 自然过期。
- 清除网络缓存时删除 search / page cache 与运行期诊断缓存，不删用户笔记、不删 AI 对话正文。

### C9 审计与诊断

- 审计记 `query_hash`、`url_hash`、provider_id、provider_kind、时间、失败类型、citation metadata。
- 不存完整 query、完整 URL、完整网页正文、完整笔记内容、大段 raw provider response、API key、token、password、cookie。
- 不新增长期 `web_evidence_ledger` 表；优先复用并扩展现有 web cache / tool audit metadata。
- 不新增长期 `web_provider_health` 表；provider health 为内存状态，持久诊断复用现有审计或 MCP 诊断缓存。
- 普通 AI 面板不展示搜索流水账。provider 失败、超时、缓存命中、MCP 调用错误等诊断放在管理中心 `AI -> 联网与证据`。

### C10 证据 UI

- 不新增"联网摘要"范式、独立 evidence 页面或 Web Evidence 工作台。
- 复用现有 AI 面板证据包展示联网证据与冲突；联网证据只是证据包的一类来源，不成为新的 UI 范式。
- 复用证据包现有详情按钮打开临时 tab；临时 tab 是短生命周期详情容器，不是新的持久导航页。
- 临时 tab 只展示证据和冲突：来源标题、canonical URL 的安全显示、短 excerpt、引用 label、冲突组、冲突说明、必要上下文。
- 临时 tab 不展示 provider 过程流水：不展示 provider 排序、调度步骤、缓存命中、fetch backend、extraction method、raw response、失败重试日志。
- provider 过程诊断只进入管理中心 `AI -> 联网与证据`，供排障使用；普通写作/问答用户不需要看到这些内部过程。
- AI 回答仍使用 Markdown 内嵌引用。

### C11 封闭平台边界

默认不支持：

- 微信搜一搜 / 公众号登录态
- 付费数据库
- 验证码
- 登录后网站
- 私有 SaaS
- paywall

允许路径：

- 用户粘贴内容
- 用户上传文件
- 用户提供公开可访问 HTTPS URL
- 未来单独设计授权浏览能力

## 前端 / IPC 同步

本设计触及 IPC 与 TS 类型，必须同步 `src/types/ipc.ts`、`src/lib/ipc.ts`、`src/types/ai.ts`：

- 删除公开外部 skill 安装 IPC：`skillsInstall` / `skillsPrepareWorkspace` / `skillsMigrateLegacy` 等。
- 删除 SkillHub / URL / Git / local import 相关类型、文案、确认流。
- 删除旧 MCP 管理平台 IPC 或改造成最小联网 Provider 配置 IPC。
- 保留 `web_search` agent 工具语义；删除旧 fetch agent tools。
- 扩展 `WebEvidenceItem` / DTO，携带 provider、trust、retrieval、conflict metadata。
- 管理中心拆分 `AI -> Skills` 与 `AI -> 联网与证据`。

## 迁移与兼容

- migration 删除旧外部 Skills 与 skill runtime 平台 schema。
- migration 删除旧 MCP 管理平台 schema，重建最小联网 Provider 配置与映射 schema。
- migration 调整 `search_cache` / `web_page_cache`，落地 vault / provider / config / broker version 隔离字段。
- 每条 migration 配 down 回滚脚本，放 `src-tauri/migrations/`，按版本号命名。
- 不迁移旧外部 skill 为新 skill；用户手动清除。
- 不保留旧 fetch tool wrapper；同一轮交付必须达到 broker 搜索 + 抓取闭环后删除旧工具，不能把旧工具作为后续技术债留下。
- 不新增证据 UI 持久实体；联网证据复用 session evidence / context packet / 现有证据包详情数据流。

## 测试计划

### Skills 收口

- 新 skill 只能是 prompt-only。
- URL / Git / Registry / 拖拽外部安装入口不存在。
- 旧外部 skill 残留不会激活、不会注入 prompt、不会导致崩溃。
- scope 写在 `SKILL.md`，数据库只保存确认 hash / 启用状态。
- 创建确认后默认启用。
- 修改 `SKILL.md` 或 scope 后 hash 变化，skill 自动停用并要求重新确认。
- skill 读上下文与 PatchProposal 使用同一 scope。
- PatchProposal 超出 scope 时被拒绝且不进入用户确认。
- `skill.mcp_bridge`、`process.*`、`secret.*` 等旧词汇在目标态 runtime 不再可达。

### MCP Provider 收口

- 未显式映射的 MCP tool 不会被 broker 使用。
- 显式 `web.search` / `web.fetch` 映射可被 broker 调度。
- Agent catalog 不含 MCP capability call 或 MCP 管理工具。
- AnySearch 样例不默认启用、不含凭据。
- Provider 配置不保存明文 API key / token。
- 旧 MCP registry / inventory / health 长期表不再作为目标态数据源。

### 网络收口

- 联网关闭时无 native / MCP / model-provider 出站调用。
- `web_search` 可处理普通 query 与明确 URL 深读。
- `fetch_web_page` / `readability_fetch` / `web_fetch_batch` / `rendered_fetch` 不在 catalog、policy、dispatch、confirmation、model prompt 中出现。
- top-2 provider 并发结果会合并，超时或失败才放弃一路。
- 同 URL 多来源命中保留 provider metadata。
- 来源冲突被标记，并进入 AI 面板证据包 / 证据详情临时 tab。
- 证据详情临时 tab 只展示证据与冲突，不展示 provider 调度、缓存、抓取、失败重试等过程流水。
- `web_evidence_items_to_packets` 携带真实 provider，不硬编码 DuckDuckGo。
- cache 按 vault / provider / config / broker version 隔离。
- 审计不存完整 query、URL、网页正文、笔记内容、凭据。
- provider 诊断出现在管理中心，不污染普通证据 UI。

## 假设

- 用户接受破坏性收口：旧外部 skill 与旧 MCP 管理平台 schema 可以删除。
- 用户会手动清除历史外部 skill；实现只保证残留安全不可用。
- MCP runtime transport 能力保留，但持久模型重塑为最小联网 Provider 模型。
- Broker 完成搜索 + 抓取闭环后再删除旧 fetch agent tools。
- self-improvement / Proactive Agent 由 harness 原生内部集成，单独立项。

## 后续单独立项

- self-improvement / Proactive Agent：harness 原生内部集成。
- 授权浏览能力：登录态、cookie、私有 SaaS、付费数据库等高风险联网能力。
- 更强 research workflow：预算策略、长任务中断恢复、持续收集等，仅在笔记核心需求明确后设计。
- 更多 MCP Provider 模板：若确有用户需求，再逐个评估，不回到 curated provider 池。
