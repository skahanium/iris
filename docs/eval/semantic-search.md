# 检索评测

> 评测对象是端到端 AI 检索，而非单独的 embedding 相似度。版本范围见 [ROADMAP.md](../../ROADMAP.md)。

## 当前基线与 v1.2.6 目标

现有基线使用 fastembed `AllMiniLML6V2`（384 维），普通语义搜索可以在 sqlite-vec 不可用时走 Rust cosine 路径；AI retrieval broker 另有 FTS、链接、锚点和法规候选。该基线不能证明 broker 的范围过滤、证据 span/hash 或排序质量。

v1.2.6 将以 BGE-small-zh-v1.5 与 Rank v2 完成强制索引迁移，评测从 `hybrid_retrieve → Rank → scope → ContextPacket` 全链路执行。完整设计与验收门槛见 [RAG 优化设计](../specs/v1.2.6-rag-optimization.md)。

## Fixture 与标签

`fixtures/rag-v2-vault/` 将包含 48 篇合成 Markdown 笔记，覆盖相近主题、长文、精确法规、tags/aliases、链接、多文档任务和干扰项。标签集共 60 条：

- 20 条语义硬负例；
- 10 条关键词/精确命中；
- 10 条 tags/aliases；
- 10 条链接/多文档；
- 10 条无答案。

fixture 只用于测试，不包含真实用户笔记或秘密。旧 `semantic-vault` 是 v1.2.5 历史基线，待 v2 fixture 落地后整体替换，不再扩充。

## 指标与发布门槛

每次评测保存机器、commit、模型、索引状态、查询标签和结果 JSON。固定基线为 `docs/eval/results/v1.2.5-hybrid.json`。

| 指标                           | v1.2.6 门槛                  |
| ------------------------------ | ---------------------------- |
| scope 泄漏                     | 0                            |
| ContextPacket span/hash 有效性 | 100%                         |
| 候选 Recall@30                 | ≥ 0.95                       |
| Recall@5                       | ≥ 0.80                       |
| 无答案 false-positive rate     | ≤ 0.10                       |
| nDCG@10、MRR@10                | 相对 v1.2.5 各提升 ≥ 0.05    |
| 任一标签子集回退               | 不超过 0.02                  |
| warm p95                       | 不劣于基线 25%，目标 ≤ 500ms |

评测失败不得以“模型下载、sqlite-vec 未启用或候选不足”跳过并宣称通过；必须明确记录降级状态和失败原因。
