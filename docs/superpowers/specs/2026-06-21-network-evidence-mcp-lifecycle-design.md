# Iris 网络能力、MCP 地基与 AI 联网生命周期设计

> Superseded by [2026-07-01-iris-reign-in-design.md](./2026-07-01-iris-reign-in-design.md).
> This file is retained for historical context only and must not be used as the implementation target.

日期：2026-06-21

## 摘要

Iris 需要把联网搜索、网页抓取、MCP 工具生态、缓存、安全、引用和 AI 会话生命周期作为一个整体系统设计，而不是继续把 `search_web`、`fetch_web_page`、MiniMax Token Plan、DuckDuckGo fallback 和后续 MCP provider 分散在不同 workflow 里。

本设计同时建设两套基础设施：

- `McpHostRuntime`：可靠、安全、可审计的 MCP 客户端和运行时，用来接入 DDG、Brave、Jina、Firecrawl、SearXNG、Tavily、Exa 等搜索/抓取 MCP server。
- `WebEvidenceBroker`：Iris 内部唯一的网络搜索、抓取、缓存、证据注入、引用和审计能力层。

Agent 不直接调用 MiniMax、DDG、Jina、Firecrawl 或任意 MCP tool。Agent 只向 `WebEvidenceBroker` 提交任务意图、查询、预算、引用需求和必要约束；broker 再选择 native provider、MCP provider、用户自配 provider 或自托管 provider 执行。

底部栏联网开关是所有普通网络证据能力的总闸。联网关闭时，native provider、MCP 网络工具、模型厂商 web search、网页抓取、网页下载和渲染抓取都不能出站。

## 目标

- 让 Iris 具备开箱即用的免费/低成本联网搜索兜底能力。
- 允许用户接入强力付费第三方搜索/抓取服务，但不要求用户必须配置 key 才能使用基础联网能力。
- 把 AnySearch Free、Jina、DDG、MiniMax Token Plan 等低成本路径纳入统一 provider 池。
- 把 DDG MCP、Brave MCP、Jina MCP、Firecrawl MCP、SearXNG MCP、Tavily MCP、Exa MCP 等生态接入能力作为本阶段地基，而不是远期附加项。
- 让所有 provider 输出统一的结构化搜索结果和证据项，废弃“中文文本块再反解析”的旧链路。
- 把网络缓存和 AI 会话、context cache、packet cache、conversation memory、笔记索引彻底隔离。
- 外部网页内容默认不可信，只以片段化证据进入模型上下文。
- 兼顾效率：通过分层并发、短 TTL、LRU、provider health、circuit breaker 和片段化注入减少延迟、token 和重复请求。

## 非目标

- 不构建自己的通用搜索引擎索引。
- 不承诺免费、稳定、高质量、无第三方依赖四者同时满足。
- 不默认抓取微信搜一搜、公众号登录态、验证码页、付费库、登录后内容或私有 SaaS 页面。
- 不把 MCP server 直接暴露成普通 agent 工具大杂烩。
- 不让 MCP tool 绕过 Iris 的联网开关、权限确认、缓存、审计和证据归一化。
- 不把网页正文自动写入用户笔记、向量索引或长期 AI 记忆。

## 当前问题

当前 Iris 的联网能力存在几类结构性问题：

- MiniMax Token Plan web search 在语义上过于接近主搜索能力，而不是普通 provider。
- DuckDuckGo fallback 主要依赖 Instant Answer 或 HTML 解析，稳定性和覆盖范围有限。
- `search_web` 返回人类可读中文文本块，`evidence_mixer` 再按字符串反解析，结构脆弱。
- `fetch_web_page`、`readability_fetch`、`rendered_fetch`、`web_fetch_batch` 等工具语义分散。
- workflow 内存在直接调用搜索的路径，agent 工具路径、写作/引用/文档 workflow 路径和旧 LLM prompt-prefix 路径没有统一生命周期。
- 项目目前几乎没有可执行 MCP runtime。`skill.mcp_bridge` 是 planned/preflight 口径，不能假设 MCP provider 已可用。
- `search_cache` 和 `web_page_cache` 只是基础 SQLite 明文 TTL 缓存，不足以支撑强联网能力所需的 vault/provider/config 隔离、审计脱敏和安全生命周期。

## 总体架构

系统分为四层：

1. Agent 决策层
   - 判断是否需要联网。
   - 指定任务模式、预算、引用需求和查询意图。
   - 不直接选择具体 provider，不直接调用 MCP tool。

2. `WebEvidenceBroker`
   - Iris 唯一产品语义联网层。
   - 负责搜索规划、provider 调度、降级、并发、归一化、缓存、证据筛选、引用和审计。

3. Provider Adapter 层
   - Native provider adapter。
   - MCP provider adapter。
   - Custom provider adapter。
   - 所有 adapter 输出同一种结构化结果。

4. `McpHostRuntime`
   - 负责 MCP server 注册、启动、工具枚举、工具调用、安全边界、日志和生命周期。
   - MCP 工具结果先进入 provider adapter，再进入 broker，不直接给 agent 消费。

## McpHostRuntime

MCP 是本阶段核心基础设施。

### MCP Server 注册表

每个 MCP server 必须进入 Iris registry，记录：

- `server_id`
- 显示名称
- 来源：official、curated、user-installed、local-dev、untrusted
- transport：`stdio` 或 Streamable HTTP
- 启动命令或 HTTP URL
- enabled 状态
- trust profile
- credential binding id
- declared capabilities
- allowed tools
- denied tools
- last health status
- last error
- created_at / updated_at

未知 MCP server 默认不能自动启用。未知 MCP tool 默认拒绝。

### Transport 支持

必须支持：

- `stdio`：本地 MCP server。
- Streamable HTTP：远程、自托管或企业内部 MCP server。

不把任意 command string 交给 shell 执行。`stdio` server 注册时必须保存结构化 command 和 args，禁止隐藏 shell wrapper 和宽泛任意 shell 字符串。

### Server 生命周期

`McpHostRuntime` 管理：

- register
- validate
- start
- health check
- `tools/list`
- `tools/call`
- timeout
- cancel
- restart
- stop
- disable
- uninstall

Repeated failure 后 server 自动降级为 unhealthy，必要时自动 disable，并在管理中心显示原因。

### Tool Discovery

工具发现读取：

- tool name
- input schema
- description
- annotations
- result schema 或 result shape

MCP annotations 只能作为不可信提示。Iris 不根据 MCP server 自报 annotations 自动授予权限。

### Tool Execution

每次 MCP `tools/call` 必须有：

- timeout
- cancellation
- stdout/stderr cap
- output size cap
- JSON parse / schema validation
- sanitized logging
- failure normalization

失败类型至少包括：

- server unavailable
- tool not found
- schema mismatch
- timeout
- output too large
- auth missing
- auth failed
- network denied
- policy denied
- invalid response

### Stdio Server 安全

本地 MCP server：

- 用户必须批准注册。
- UI 显示 command 和 args。
- 不允许静默执行 `npx`、shell heredoc、宽泛 shell string 或未知脚本。
- 限制 env。
- 凭据必须通过 Iris credential binding 显式绑定，不能随意注入环境变量。
- 日志脱敏。
- timeout 和输出上限强制执行。
- 崩溃后标记 unhealthy。
- repeated failure 后自动 disable。

### HTTP MCP 安全

远程 MCP server：

- 默认必须 HTTPS。
- localhost 只允许显式 dev mode。
- 校验 redirect。
- 拒绝 private IP、loopback 非 dev、metadata endpoints 和明显 DNS rebinding 风险。
- 禁止 token passthrough。
- 凭据按 server/provider 绑定。
- 每个 server 都有 timeout 和 output cap。

### Capability Mapping

MCP tools 映射到 Iris capability：

- `web.search`
- `web.fetch`
- `web.rendered_fetch`
- `web.find_in_page`
- `web.download`
- `web.extract_markdown`
- `credential.required`
- `paid.api`
- `free.best_effort`

映射关系来自 Iris registry 和 curated profile，不来自 MCP server 自报权限。

## WebEvidenceBroker

`WebEvidenceBroker` 是唯一面向产品语义的联网层。

它负责：

- query planning
- provider selection
- provider health
- provider fallback
- limited parallelism
- search result normalization
- page fetch normalization
- evidence item generation
- cache reads/writes
- prompt-injection boundary
- citation metadata
- audit metadata
- retention cleanup

### Provider 类型

Native providers：

- AnySearch Free
- Jina Search / Reader HTTP
- DuckDuckGo Instant Answer
- DuckDuckGo HTML 或 Lite fallback
- MiniMax Token Plan web search
- local static HTTPS fetch
- vertical open APIs

MCP providers：

- DDG MCP
- Brave Search MCP
- Jina MCP
- Firecrawl MCP
- SearXNG MCP
- Tavily MCP
- Exa MCP

Custom providers：

- 用户自配付费 API
- 自托管 SearXNG
- 自托管 broker endpoint
- 企业/内网搜索 endpoint

MCP provider 不能绕过 broker。它们只是 broker 后面的 adapter。

## Provider 策略

### 默认免费 / 低成本池

默认 auto pool 是 best-effort，不承诺 SLA。

AnySearch Free：

- 默认兜底候选。
- 可关闭。
- 必须健康检查。
- 不承诺稳定性、额度或地区可用性。
- 不能因为免费可用就被视为可信来源。
- 若公开文档、服务稳定性或地区可用性不足，健康检查会降低其优先级。

Jina Search / Reader：

- 用于搜索、页面 Markdown 化和正文读取补充。
- 适合把开放网页转为 LLM 友好文本。
- 受速率限制和地区可用性影响。

DuckDuckGo：

- Instant Answer 用于快速事实。
- HTML/Lite fallback 用于更宽泛搜索。
- 低稳定性承诺。

MiniMax Token Plan：

- 模型厂商附带的低成本搜索。
- 普通 provider，不再是架构中心。
- 如果用户已配置 MiniMax，可作为低成本补充。

Static HTTPS fetch：

- 受控单页抓取。
- 默认不做 JS 渲染。

### 强 Provider 池

用户配置或 MCP 启用后进入增强路径：

- Brave Search
- Tavily
- Serper
- Exa
- Kagi
- Firecrawl
- SearXNG
- custom broker

这些 provider 在配置且健康时可以排在免费 provider 前面。

### 垂直开放来源

Broker 可以按任务类型补充垂直来源：

- OpenAlex / Crossref / Semantic Scholar / arXiv：学术任务。
- PubMed / Europe PMC：生物医学任务。
- Wikipedia / Wikidata：百科事实。
- GDELT：新闻和事件。
- GitHub / Stack Exchange / package registry APIs：技术任务。
- RSS/Atom feeds：指定媒体、博客、机构公告。

这些不是通用 web search 替代品，只在相关任务中补充使用。

## Agent 联网生命周期

### 1. 请求进入

输入可能来自：

- AI 对话
- 文档编辑区
- 选中文本
- 写作命令
- 引用核查
- 研究请求
- 文档检查

前端传入：

- message
- selected context reference
- note path/hash
- web switch state
- session id
- task intent hints

在 policy evaluation 前不得发生网络请求。

### 2. Policy Gate

Broker 检查：

- 底部栏联网开关
- task policy
- autonomy level
- tool permission profile
- MCP trust profile
- provider enabled state
- vault scope
- classified/sensitive path exclusions

联网关闭时，broker 返回结构化 no-web 结果，不调用 native provider、MCP provider 或模型厂商 web-search provider。

### 3. 是否需要搜索

Agent 可以请求 web evidence，但 broker 仍可降级或拒绝。

任务模式：

- `quick`
- `standard`
- `research`

常见映射：

- 轻量事实：`quick`
- 写作/引用辅助：`standard`
- 多来源研究：`research`

Agent 提供意图和约束，broker 负责执行规划。

### 4. 查询规划

默认规则优先：

- 去除提示噪声
- 抽取关键词、实体和主题
- 检测语言/地区
- 检测是否需要时效性
- 检测垂直领域
- 生成 1-3 个候选查询

只有以下情况才使用模型扩展查询：

- mode 是 `research`
- 第一轮结果低质量
- 用户要求对比、文献综述、政策、市场研究
- 查询语义模糊，需要替代问法

隐私规则：

- 不发送整篇笔记。
- 默认不发送完整选区上下文。
- 只发送最小查询词和必要的小段上下文。

### 5. Provider 选择

Broker 动态排序 provider，依据：

- 用户配置优先级
- provider health
- cost class
- 近期失败次数
- rate limit 状态
- locale 支持
- task mode
- 需要的来源类型
- provider trust profile

默认策略是分层并发：

- `quick`：1-2 个低成本 provider。
- `standard`：低成本 provider + 必要 fallback。
- `research`：多个 provider + 垂直来源 + 后续正文抓取建议。

不允许每次盲目调用所有 provider。

### 6. Provider 执行

每个 provider 调用必须有：

- timeout
- max results
- max response bytes
- cancellation
- rate limit bucket
- provider-specific circuit breaker
- sanitized logging
- failure classification

失败类型：

- timeout
- rate limited
- empty result
- low quality
- parse error
- auth missing
- auth failed
- network blocked
- policy denied
- provider disabled

### 7. 结果归一化

Provider 输出归一化为稳定类型。

`SearchResult`：

- id
- title
- url
- snippet
- provider_id
- provider_kind：native / mcp / custom / model_provider
- source_type
- source_rank
- published_at
- fetched_at
- locale
- score
- cost_class
- raw_result_hash
- failure_reason 可选

`FetchedPage`：

- id
- url
- canonical_url
- title
- text_excerpt
- full_text_ref
- content_hash
- provider_id
- fetched_at
- content_type
- byte_count
- truncated
- extraction_method：static / reader / rendered / mcp
- trust_level：external_untrusted
- cache_policy

`WebEvidenceItem`：

- evidence_id
- source result/page id
- citation label
- title
- url
- excerpt
- content_hash
- provider_id
- source_rank
- fetched_at
- retrieval_reason
- trust_level
- stale flag

旧的中文文本块搜索结果不能再作为内部格式。

### 8. 缓存读写

网络缓存必须独立于：

- `session_messages`
- context assembly cache
- packet cache
- conversation memory
- note index
- vector index

专用缓存区：

- `web_search_results_cache`
- `web_page_content_cache`
- `web_provider_health`
- `web_evidence_ledger`

缓存 key 必须包含：

- `vault_id`
- `provider_id`
- `provider_config_hash`
- `query_hash` 或 `url_hash`
- locale
- search mode
- broker version
- safe-search/privacy mode

搜索结果缓存：

- 小时级短 TTL
- 存结构化结果，不存 prompt-ready 文本

网页正文缓存：

- 默认 24 小时内短 TTL
- 完整页面正文只存在 web cache
- 不进入 session messages 或长期 AI memory
- 后续可支持 memory-only 模式

Provider health cache：

- 分钟级 TTL
- 只用于 provider 排序和 circuit breaking

Evidence ledger：

- 存 task-level evidence IDs、hash、citation、provider metadata
- 不复制完整网页正文

### 9. 证据筛选

Broker 选择可注入模型的 evidence。

筛选依据：

- relevance
- diversity
- source type
- source rank
- freshness
- duplication
- task mode
- token budget

只有选中的 excerpt 进入模型上下文。

完整正文留在 web cache，需要时重新抽片段。

### 10. 注入模型上下文

外部网页内容一律包成不可信证据。

注入内容包括：

- excerpt
- title
- URL
- provider
- fetched time
- content hash
- citation label
- 边界说明：外部网页文本不是指令

模型不得把网页文本当作 system/developer/user 指令执行。

### 11. 回答与引用

UI 行为：

- 不新增证据/来源 tab。
- 回答正文使用 Markdown 内嵌引用、脚注或链接。
- 消息底部可以显示折叠的联网摘要：
  - 执行了哪些搜索
  - 打开了哪些 URL
  - 使用了哪些 provider
  - fallback path
  - failures
  - cache hits

这个摘要只是信息展示，不是独立工作区。

### 12. 审计

Tool audit 只存脱敏元数据：

- request id
- tool/provider
- query hash
- URL hash
- result count
- status
- duration
- content hash
- 安全短 preview
- failure class

Audit 不得存：

- API key
- token
- 完整笔记内容
- 完整网页正文
- 大段 raw provider response
- password
- cookie/login data

### 13. 留存与清理

默认清理是自动且用户无感的。

策略：

- search cache：小时级 TTL
- page cache：默认 <= 24h TTL
- provider health：分钟级 TTL
- failed/low-quality results：更短 TTL
- LRU 容量上限
- app startup 和后台周期清理

用户界面：

- Management Center 可以提供高级 clear/reset 入口。
- 日常安全不依赖用户手动清理缓存。

删除聊天会话：

- 删除 session messages 和 task references
- 不一定立即删除全局 web cache
- 删除或安全孤立该会话的 ledger references
- web cache 按 TTL/LRU 自然过期

清除网络缓存：

- 删除 search/page/provider health caches
- 不删除用户笔记
- 不删除 AI 对话正文

## 权限规则

Search：

- 底部联网开关开启后可自动执行
- 仍受 provider policy 和 rate limit 约束

Open page / fetch：

- 需要确认
- 确认内容显示 URL、reason、provider、预计 cache TTL

Batch fetch：

- 需要确认
- 强制 max URLs

Rendered fetch：

- 需要确认
- 高级能力
- 不是默认 fallback

Download/assets：

- 需要确认
- 与普通 evidence collection 分离

Login/cookie/auth browsing：

- 不属于默认联网能力
- 未来必须单独设计 authorized browsing

## 封闭平台边界

默认不支持：

- 微信搜一搜
- 公众号登录态页面
- 付费数据库
- 验证码页面
- 登录后网站
- 私有 SaaS 页面
- paywall 内容

允许路径：

- 用户粘贴内容
- 用户上传文件
- 用户提供可访问 URL
- 未来授权浏览能力

## 性能设计

效率机制：

- 分层并发
- provider health cache
- provider circuit breaker
- 短 TTL 复用
- page content LRU
- excerpt-only model injection
- query planning rules before model expansion
- 足够高质量结果到达后取消剩余请求
- per-mode latency budgets

预算：

- `quick`：2-4 秒
- `standard`：6-10 秒
- `research`：20-60 秒

Provider 竞速行为：

- quick mode：第一个可接受结果可以满足请求。
- standard mode：等待有限来源多样性。
- research mode：在预算耗尽前持续收集。

## UI

底部栏：

- 一个联网总开关
- 紧凑状态

AI 消息：

- Markdown 内嵌引用/链接
- 消息底部可折叠联网摘要

Management Center：

- 免费兜底 provider。
- AnySearch/Jina/DDG/MiniMax 健康状态。
- MCP server 注册表。
- curated MCP 推荐。
- 付费 provider 配置。
- 自托管/custom endpoint。
- rendered fetch 高级设置。
- 隐私与留存说明。

不新增证据/来源 tab。

## 迁移与兼容

需要收敛的现有路径：

- `search_web`
- `fetch_web_page`
- `readability_fetch`
- `rendered_fetch`
- `web_fetch_batch`
- workflow 内直接 search 调用
- prompt-prefix web injection

迁移规则：

- legacy tools 暂时保留为兼容 wrapper。
- wrapper 内部调用 broker。
- MiniMax-specific search cache 变为 provider-specific cache。
- 旧 `search_cache.body` 文本格式不再用于新的 structured evidence。
- web page cache 增加 provider/config/vault/broker key 隔离。
- direct prompt prefix injection 改为 evidence packet injection。

## 实施顺序

1. 定义数据模型：
   - `SearchResult`
   - `FetchedPage`
   - `WebEvidenceItem`
   - `ProviderHealth`
   - `ProviderFailure`
   - cache key types

2. 建设 MCP 地基：
   - registry
   - stdio transport
   - HTTP transport
   - tool discovery
   - tool call
   - trust profile
   - permission integration
   - audit integration

3. 建设 broker core：
   - provider interface
   - provider scheduler
   - normalization
   - cache read/write
   - lifecycle audit

4. Native provider adapters：
   - AnySearch Free
   - Jina HTTP
   - DDG
   - MiniMax Token Plan
   - static fetch

5. MCP provider bridge：
   - generic MCP provider adapter
   - DDG/Jina/SearXNG/Firecrawl/Brave/Tavily/Exa 的 curated profiles

6. Agent integration：
   - query planning
   - mode budgets
   - broker calls
   - evidence injection
   - citation generation

7. UI integration：
   - bottom switch enforcement
   - message citations
   - collapsed network summary
   - Management Center MCP/provider settings

8. Cleanup：
   - 删除 workflow direct search calls。
   - 退役 text-block parsing path。
   - 迁移旧测试到 broker contracts。

## 测试计划

### MCP Runtime

- stdio server 可以启动、枚举工具、调用工具和停止。
- HTTP server 可以连接、枚举工具和调用工具。
- timeout 会取消工具调用。
- 崩溃的 server 会被标记为 unhealthy。
- disabled server 不能被调用。
- unknown tools 会被拒绝。
- output cap 会被强制执行。
- stderr/logs 会被截断并脱敏。

### MCP Security

- token passthrough 会被拒绝。
- private IP HTTP server 在非 dev mode 下会被拒绝。
- redirect-to-private 会被拒绝。
- unknown capabilities 会被拒绝。
- 联网开关关闭时，MCP network tools 会被阻断。
- 高风险 MCP tool 需要确认。

### Broker

- 联网关闭时，不发生 native/MCP/model-provider 出站调用。
- quick/standard/research 预算被正确遵守。
- provider failure 会触发 fallback。
- provider health 会影响排序。
- duplicate URLs 会被去重。
- 旧 MiniMax-specific path 不再硬编码。

### Cache

- search/page/provider cache 按 vault/provider/config 隔离。
- session messages 不包含完整网页正文。
- context cache 不包含完整网页正文。
- TTL 过期行为正确。
- LRU 淘汰行为正确。
- failed/low-quality entries 使用更短留存。

### Prompt Safety

- 网页提示注入始终停留在“外部证据”边界内。
- 注入上下文包含“不可信外部内容”边界说明。
- 模型默认只接收 excerpt，不接收完整缓存页面。

### Permissions

- 联网开关开启时 search 可自动运行。
- open page 需要确认。
- batch fetch 需要确认。
- rendered fetch 需要确认。
- download 需要确认。

### UI

- 底部栏仍只有一个联网总开关。
- 不新增证据/来源 tab。
- 内嵌引用可正常渲染。
- 折叠联网摘要可正常渲染。
- Management Center 可以禁用免费兜底。
- Management Center 可以禁用 MCP server。

## 假设

- MCP 是本阶段核心基础设施，不是未来附加项。
- MCP 能减少 provider 接入成本，但不能替代 Iris 的 broker、policy、cache、audit 或 UI。
- 免费 provider 都是 best-effort，必须动态健康检查。
- 强搜索质量来自用户配置的付费 provider、MCP provider、自托管服务或模型厂商附带搜索。
- Iris 不承诺访问封闭平台、付费内容或登录态内容。
