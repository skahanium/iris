# Tools、Skills、MCP、联网与 Provider 规格

## 1. 稳定 Capability 层

Harness 和 executor 只能请求稳定 capability，不直接依赖具体工具或 Provider 名称。示例：

```text
vault.search
vault.read_document
web.search
web.fetch
note.propose_patch
note.apply_patch
runtime.inspect
```

每个已接受 Run 在首次策略通过后必须持久化一份按字典序规范化的
`allowed_capabilities` 快照。模型可见工具面、每一次工具 gate、确认后
冻结计划的执行都只能消费该快照；同一 Run 再次求值若产生不同集合必须
fail-closed，不能因当前设置、effort 或 access level 变宽。

Capability Resolver 返回满足当前安全域、权限和运行环境的具体实现。不存在实现时返回结构化 unsupported，不允许模型猜测替代工具。

## 2. Tool Catalog 与执行流水线

每个工具目录项必须声明：

- 稳定名称、Capability ID 和版本。
- 输入/输出 JSON Schema。
- 访问级别、风险、是否可并行、是否可取消。
- 可能读取或修改的资源范围推导器。
- 最大输出和超时。
- 是否产生 Evidence。

工具与 capability 的对应必须是按工具名精确声明的一对一合同，禁止以
`WriteMarkdown`、`Durable` 等粗粒度属性推导整组工具权限。例如
`note.apply_patch` 只能暴露选区插入/替换，不能同时暴露 memory、schedule、
external-fs、vault 管理或 Git 写入。

唯一执行顺序：

```text
catalog lookup
→ schema validation
→ effect derivation
→ policy decision
→ confirmation validation
→ dispatch
→ output validation/truncation
→ evidence registration
→ audit/event
```

Tool Dispatcher 不接收“已经安全”的布尔值；它接收不可伪造的 policy/confirmation token，并在 dispatch 前重新验证关键参数。

确认卡片必须展示规范化、长度受限的实际目标标识与摘要；不得只展示
“目标 1”之类无法让用户判断范围的占位文本，也不得展示原始 tool args。

## 3. 并发

- 只有目录中显式标记为 parallel-safe 的只读工具可以并发。
- 所有写工具串行执行。
- 同一路径的读取与写入不得交错。
- 子 Agent 共享父 Run 的并发、token、工具和 Web 总预算。
- 取消 Run 时必须传播到 Provider 请求、工具 future 和子 Agent。

## 4. Skills

Skills 是用户确认后启用的 prompt-only 行为包。

### 注册与缓存

- vault 激活时扫描一次；`skills_list` 是显式刷新边界。确认写入的单个 Skill 直接更新缓存和索引，不另起扫描。
- 以 canonical vault path 为 key 缓存完整、已解析的 `SkillEntry`；缓存包含 manifest、触发器、prompt 片段、内容 hash 和确认状态。
- 同一刷新边界同步重建当前 vault 的 `skill_activation_index`，其中只持久化 description 与声明 keywords，绝不持久化完整 Skill 指令正文。
- 每个 Run 只读取内存缓存和 activation index；禁止在 Run 中遍历文件系统、刷新缓存或重新生成 embedding。

### 激活

优先级：

1. 用户明确点名或 UI 明确选择。
2. manifest 中的精确触发器和 capability 条件。
3. 高置信自动匹配。

默认一个主 Skill，最多一个辅助 Skill。未达到证据阈值时不激活；无显式点名、触发器、任务/来源/索引匹配的 Skill 不得因“已启用”而自动注入。禁止给所有已启用 Skill 固定正基础分，也禁止用 legacy scene 二次重排。

当前实现将主、辅两个 Skill 的正文各硬截断到 `4,000` 个 Unicode 字符（合计最多 `8,000`），使 Run 在未绑定 provider tokenizer 时仍有确定上界；接入 provider 输入预算后，进一步以 `min(可用输入预算的 10%, 8k tokens)` 收紧，超限时优先保留显式 Skill，再按 manifest 优先级裁剪，并在运行详情中报告。

### 边界

- Skill 不能执行脚本、安装依赖、定义任意 MCP server 或直接调用工具。
- Skill 不授予 capability，也不因 freshness 自动启用联网；唯一能力来源是 Policy Engine 持久化的 Run 授权快照。
- Skill 内容按受控行为说明注入，不包含来自第三方资料的未标记 system 指令。

## 5. MCP

Iris 不提供通用 MCP 直通。首版只允许 MCP 作为 `web.search`、`web.fetch` 的类型化后端。

Adapter 配置必须声明：

- transport、credential refs、tool mapping 和 Schema mapping。
- 健康状态、超时、输出限制和 provider config hash。
- 搜索结果及抓取结果到 Iris 证据类型的确定性转换。

启动诊断异步完成，不阻塞简单问答。进入 ToolLoop 时必须冻结已选 Web Provider 及其 mapping hash；若当时没有 Provider，也冻结为不可用，Run 中途新增或改配 Provider 不得改变该决定。MCP 资源、prompt 和未映射工具不得进入模型上下文。

HTTP MCP 必须使用 HTTPS（仅显式开发 localhost 例外）、禁用重定向，并在连接前解析 hostname、拒绝私网/metadata 地址后把解析结果固定到本次 reqwest client，防止 DNS rebinding。HTTP 响应先检查 `Content-Length`，再对 chunked body 在交给 JSON/SSE 解码器前累计上限；stdio 在 JSON-RPC 行帧交给解码器前执行同一上限。两者超限都归类为 `output_too_large`，不回显远端正文。stdio 复用池最多保留 `8` 个空闲会话，按确定性 LRU 驱逐；同一启动 fingerprint 的初始化和调用必须经单一 profile gate 串行化，禁止并发首调用重复 spawn/handshake；协议协商只接受宿主 allowlist 中已验证的版本。

## 6. 联网语义

- 联网开关是 `web.search` 的唯一授权源：只有 Intake 可把开关结果写入 immutable Execution Envelope；关闭时不创建该 capability、不向模型暴露 Web 工具，任何 Native/MCP Web 调用均被拒绝。即使后续调用方显式请求 `web.search` 或 `web.fetch`，只要它不在该 envelope 中，Policy Engine 就必须拒绝；freshness、Skills、提示词和 ChildRun 都不能增权。
- 开关开启：对外部可核验事实至少为 `web_preferred`。
- 最新、当前、价格、规则、人物职位、URL、明确“搜索/核实”等请求为 `web_required`。
- 创作、纯改写、只基于用户文本和明确“只用本地”的请求不强制联网。
- `web_required` 未取得证据时不得把未核实内容表述为已核实事实。
- Web 搜索/抓取结果统一进入 Evidence Ledger，最终回答引用实际来源页面而非搜索结果页。

## 7. LLM Provider

Provider Router 输入为能力要求，不接受旧 scene：

```text
endpoint family
streaming/tools/vision/reasoning support
input/output budget
security domain
privacy preference
availability and recent health
```

### 凭据

- 主候选和 failover 候选都必须在实际 dispatch 前正确 hydrate credential。
- 未被实际尝试的候选不得解密 Key。
- 解密值使用 `Zeroizing`，不得进入日志、错误或 checkpoint。

### 故障转移

允许：连接超时、连接失败、429、可重试 5xx 和明确临时不可用。

禁止：401/403、请求 Schema 错误、上下文过长、内容/权限策略拒绝、用户取消和安全域不匹配。

切换后必须记录实际 Provider、模型、错误分类和候选序号；不向用户暴露敏感 endpoint 或凭据详情。

### Adapter 合同

OpenAI-compatible 与 Anthropic 等差异必须封装在 Provider Adapter：工具调用、流式 usage、reasoning、finish reason、错误分类和取消都需要合同测试。Harness 不得出现按 Provider 名称分支的业务规则。

## 8. 性能关键路径

不得进入简单问答关键路径：

- Skill 全量扫描或 embedding。
- MCP 健康探测。
- 重复 Context Assemble。
- 未使用 Provider 的凭据解密。
- 固定 planning/reflection/final 三调用链。

允许并发的准备工作：Envelope/Policy、Provider route、必要的轻量 Context 计划。接受事件必须先于这些耗时步骤发出。
