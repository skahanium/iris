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

Capability Resolver 返回满足当前安全域、权限和运行环境的具体实现。不存在实现时返回结构化 unsupported，不允许模型猜测替代工具。

## 2. Tool Catalog 与执行流水线

每个工具目录项必须声明：

- 稳定名称、Capability ID 和版本。
- 输入/输出 JSON Schema。
- 访问级别、风险、是否可并行、是否可取消。
- 可能读取或修改的资源范围推导器。
- 最大输出和超时。
- 是否产生 Evidence。

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

## 3. 并发

- 只有目录中显式标记为 parallel-safe 的只读工具可以并发。
- 所有写工具串行执行。
- 同一路径的读取与写入不得交错。
- 子 Agent 共享父 Run 的并发、token、工具和 Web 总预算。
- 取消 Run 时必须传播到 Provider 请求、工具 future 和子 Agent。

## 4. Skills

Skills 是用户确认后启用的 prompt-only 行为包。

### 注册与缓存

- 启动时扫描一次，文件变化后增量刷新。
- 缓存 manifest、触发器、prompt 片段、内容 hash 和确认状态。
- 禁止在每轮请求中遍历文件系统或重新生成 embedding。

### 激活

优先级：

1. 用户明确点名或 UI 明确选择。
2. manifest 中的精确触发器和 capability 条件。
3. 高置信自动匹配。

默认一个主 Skill，最多一个 manifest 声明兼容的辅助 Skill。未达到阈值时不激活。禁止给所有已启用 Skill 固定正基础分，也禁止用 legacy scene 二次重排。

Skill prompt 总预算应取模型可用输入预算的 10% 与 8k tokens 中较小值；超限时优先保留显式 Skill，再按 manifest 优先级裁剪，并在运行详情中报告。

### 边界

- Skill 不能执行脚本、安装依赖、定义任意 MCP server 或直接调用工具。
- Skill 声明所需 capability，但 Policy Engine 可以拒绝。
- Skill 内容按受控行为说明注入，不包含来自第三方资料的未标记 system 指令。

## 5. MCP

Iris 不提供通用 MCP 直通。首版只允许 MCP 作为 `web.search`、`web.fetch` 的类型化后端。

Adapter 配置必须声明：

- transport、credential refs、tool mapping 和 Schema mapping。
- 健康状态、超时、输出限制和 provider config hash。
- 搜索结果及抓取结果到 Iris 证据类型的确定性转换。

启动诊断异步完成，不阻塞简单问答。运行中选择已启用且健康的一个后端；瞬态失败可按同 capability 规则切换。MCP 资源、prompt 和未映射工具不得进入模型上下文。

## 6. 联网语义

- 开关关闭：`offline`，任何 Native/MCP Web 调用都被 Policy Engine 拒绝。
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
