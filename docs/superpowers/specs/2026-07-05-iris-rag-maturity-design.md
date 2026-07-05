# Iris RAG Maturity Design Spec

日期：2026-07-05
状态：Ready for implementation planning
范围：本地 Markdown RAG、语义检索、混合检索、证据包、轻量研究闭环

## 1. 背景

Iris 已经具备 RAG 的主要骨架：Markdown vault 索引、FTS5、fastembed chunk embeddings、sqlite-vec optional 路径、`retrieval_broker` 多路检索、`ContextPacket`、`EvidenceLedger`、WebEvidenceBroker 与 research workflow。当前问题不是“没有 RAG”，而是成熟度不足：

- 语义搜索路径存在 SQL 字符串错误，可能导致 sqlite-vec 与 cosine fallback 均不可用。
- `retrieval_broker/vector.rs` 读取的 `chunks` 字段与当前 schema/indexer 写入字段不一致，AI 助手的 vector chunk 证据可能无法返回。
- chunk 仍是 v0.1 级别，缺少 heading path、source span、content hash、overlap 和稳定证据定位。
- ranking 主要依赖固定权重，缺少分数归一、文件级去重、多样性控制和可靠评测。
- research workflow 已有形态，但还没有强制把子问题、证据覆盖、冲突和证据缺口连成闭环。

本设计目标是在现有基础上优化完善，而不是重写 RAG 或把 Iris 变成通用型 agent。

## 2. 产品定位

Iris RAG 的成熟目标是“可信笔记助手 + 轻量研究助理”：

- **可信笔记助手**：围绕用户本地 Markdown 笔记回答问题、改写文稿、检查引用，并能说明依据来自哪些笔记片段。
- **轻量研究助理**：在研究任务里拆解问题、汇总本地证据、在用户授权时补充 WebEvidenceBroker 外部证据，并标出证据不足和冲突。
- **非通用 agent**：不追求自动执行开放式任务，不扩展成插件平台，不引入企业级多租户检索系统。

成熟标准：

1. 每条进入 prompt 的证据都可诊断、可定位、可引用、可评测。
2. 本地证据不足时，助手明确说不足，不把弱匹配包装成结论。
3. 研究模式可以更严谨，但普通问答保持自然，不让用户感觉被流程表格绑住。

## 3. 目标

1. 修复现有 RAG 断链，使 `search_semantic`、`search_hybrid`、`context_assemble` 都能稳定返回真实证据或明确诊断。
2. 将 chunk 从纯文本片段升级为可引用证据单元，支持 heading、span、hash、embedding model 和兼容迁移。
3. 提升混合检索质量：FTS、vector、exact、graph、template 分数统一、去重、多样性和语料角色权重。
4. 建立工程和质量指标并重的评测体系，覆盖命中、排序、无答案、引用定位和延迟。
5. 让 research workflow 输出证据覆盖和冲突状态，但 UI 不新增强制流程，不把助手变得死板。

## 4. 非目标

- 不引入 PostgreSQL、LanceDB、Qdrant 或任何外部向量数据库。
- 不把 `.md` 笔记内容存储为专有格式；SQLite 只保存可重建索引和应用状态。
- 不默认依赖云端 embedding 或 rerank。
- 不重写 AI harness、session evidence、WebEvidenceBroker 或 UI 主结构。
- 不自动修改用户 `.md` 文件；所有写入仍走现有确认和 patch preview。
- 不让 research workflow 强制每次都生成长报告或表格；严格性用于内部证据判断，表达保持自然。

## 5. 总体架构

成熟化继续沿现有链路演进：

```text
.md vault
  -> indexer
  -> files / chunks / links / tags / files_fts / chunk_embeddings / vec_chunks
  -> retrieval_broker
  -> packet_builder
  -> EvidenceLedger
  -> prompt_builder / harness / workflows
```

新增和调整只围绕四个边界：

1. **Indexing**：从 Markdown 派生稳定 chunk evidence，不改变 `.md` 权威地位。
2. **Retrieval**：每个检索层返回 packets 和 diagnostics，不用空数组掩盖错误。
3. **Ranking**：分数归一、去重、多样性和可选 reranker 挂在 broker 内部，不改变上层 IPC。
4. **Evidence**：`ContextPacket` 是进入 prompt 的唯一证据契约，所有回答和工作流引用都以 ledger 注册结果为准。

## 6. 数据模型与兼容策略

### 6.1 Chunk Evidence Contract

`chunks` 应具备以下派生字段：

- `content`: chunk 正文。
- `char_count`: chunk 字符数。
- `heading_path`: 当前 chunk 所属 Markdown 标题路径，可为空。
- `source_start`: chunk 在正文 UTF-8 字节范围起点。
- `source_end`: chunk 在正文 UTF-8 字节范围终点。
- `content_hash`: chunk 内容 hash。
- `embedding_model`: 写入 embedding 时使用的模型 id。

实施采用增量 migration，字段只追加，不删除旧字段；现有 `content` 继续作为正文列，避免引入 `text` 别名造成二义性。

### 6.2 ContextPacket Contract

进入 prompt 的本地证据必须尽量包含：

- `source_path`
- `heading_path`
- `source_span`
- `content_hash`
- `retrieval_reason`
- `score`
- `trust_level`
- `citation_label`

如果旧索引暂时缺少 span/hash，packet 可以返回，但 diagnostics 必须标注 `legacy_chunk_metadata`，便于 UI 和日志解释。

## 7. 检索与诊断

`retrieval_broker` 继续作为统一入口。每个 layer 应能报告：

- `ok`: 查询成功，可有结果或无结果。
- `empty`: 查询成功但没有命中。
- `index_not_ready`: 依赖的索引或模型未准备好。
- `schema_mismatch`: SQL 字段、表或迁移状态不符合预期。
- `query_error`: 查询、embedding、sqlite-vec 或 fallback 执行失败。

降级规则：

- FTS 可用、vector 不可用时，允许使用 FTS/exact/graph/template 返回结果，但 diagnostics 必须说明 vector 降级。
- sqlite-vec 不可用时允许 cosine fallback；fallback 超过安全 chunk 上限时返回 `index_not_ready` 或 `query_error`，不伪装成“没有结果”。
- WebEvidenceBroker 未授权时不调用外部来源；研究模式可提示“未使用外部证据”。

## 8. 排序与质量层

P1 默认不依赖新增模型即可提升质量：

- FTS 分数、vector distance、exact 命中、graph confidence、template usage 进入统一 0-1 分数域。
- 对同一文件多个 chunk 做文件级去重和相邻片段合并，避免 top-k 被同一文档淹没。
- 使用 MMR 保持结果多样性，降低“全是相似片段”的 prompt 浪费。
- `corpora.toml` 的 authority/exemplar/reference/lookup 继续影响权重，但不得覆盖用户显式 `@scope`。
- 当前笔记上下文可作为轻量 query hint，但不能让当前笔记无条件压过更相关证据。

可选轻量 reranker：

- 通过 `CapabilitySlot::Reranker` 接入。
- 默认仍为 `score-fusion`，不新增依赖也可完整运行。
- 新增 reranker 必须 AGPL-3.0 兼容、本地优先、可关闭、可回退。
- reranker 只重排候选，不直接生成答案，不改变证据来源。

## 9. 轻量研究闭环

Research workflow 的成熟目标是“证据覆盖清楚”，不是“输出格式僵硬”。

内部状态新增或强化：

- `ResearchQuestion`: 用户问题或拆出的子问题。
- `EvidenceCoverage`: 每个子问题对应的 supporting packets、missing evidence、conflicting packets。
- `ResearchFinding`: 已证实、证据不足、来源冲突、需要用户补充材料四类结论状态。

对用户的表达规则：

- 普通回答仍自然组织，不强迫输出表格。
- 研究任务可以在 evidence detail 或 artifact 中展示覆盖矩阵。
- 最终回答用自然语言说明边界，例如“你的笔记里能支持 A，但 B 没有直接证据”。
- 不因为 coverage 机制存在就让每个回答都像审计报告。

## 10. UI 与体验原则

- 默认保持现有助手体验：用户提问，助手自然回答，证据卡可展开。
- 诊断信息分层：用户看到简洁状态，开发日志和 trace 保留详细原因。
- 证据不足时给出下一步建议，但不弹强制确认框。
- `ContextPacketDrawer` 和 `EvidenceChainView` 承担透明度，不把主对话变成机械流程。
- 写作类能力继续优先给可用草稿和引用提示，严谨性体现在 evidence ledger 和 citation verifier 中。

## 11. 测试与验收

### 11.1 工程门禁

- `semantic_search` 覆盖 sqlite-vec prepare、cosine fallback、classified path 过滤和 SQL 错误回归。
- `retrieval_broker` 每个 layer 覆盖正常命中、空结果、缺表、缺列、索引未就绪。
- `packet_builder` 覆盖 scope、corpus role、long context 和 diagnostics 透传。
- `EvidenceLedger` 覆盖去重、稳定 citation、预算压缩和用户选定证据刷新。
- 前端 contract 覆盖证据包、诊断状态、无证据提示和 citation drawer。

### 11.2 质量指标

- Recall@5 / Recall@10
- MRR
- nDCG@10
- no-answer accuracy
- citation localization rate
- local retrieval latency
- index rebuild latency

### 11.3 评测集

保留现有 ignored fastembed fixture，同时新增非 ignored deterministic fixture：

- deterministic fixture 使用 fake embedding backend，不下载模型，专门锁 broker、ranking 和 evidence 逻辑。
- fastembed fixture 扩展到 distinctive query、模糊 query、同义 query、跨笔记 query、噪声 query、无答案 query。

## 12. 分阶段交付

### P0: Reliable RAG

修断链、补诊断、补回归测试。完成后现有功能应只变得更可靠，不改变用户可见交互。

### P1: Quality RAG

引入 chunk v2、ranking v2、质量指标和可选 reranker 接口。完成后检索更准，prompt 证据更少而更有用。

### P2: Research RAG

补 research coverage matrix、冲突/缺口说明和研究任务可视化。完成后研究回答更可信，但普通回答仍保持自然。

## 13. 风险与缓解

- **迁移风险**：chunk 字段只追加，不删除；旧索引可继续读，新索引重建后获得完整 metadata。
- **性能风险**：ranking v2 先在 top-N 候选内运行；reranker 可关闭；大库 fallback 有明确诊断。
- **体验风险**：诊断默认折叠，主回答保持自然，研究矩阵只在研究任务或详情视图中出现。
- **依赖风险**：P0/P1 不要求新增依赖；轻量 reranker 单独走许可检查和回退策略。
- **回归风险**：每阶段先写失败测试，修复后跑 Rust 和前端质量门禁。

## 14. 成功标准

1. 本地语义检索不再因 SQL/schema 漂移静默失败。
2. 助手上下文中的本地证据可定位到文件、标题、span 和 hash。
3. 混合检索质量指标可持续报告，并在 fixture 上较当前基线提升或保持。
4. 无答案场景能被识别，不以弱证据生成强结论。
5. 研究任务能呈现证据覆盖和冲突，但普通对话不变得僵硬。
