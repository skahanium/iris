# 检索评测

> 评测对象是端到端 AI 检索，而非单独的 embedding 相似度。版本范围见 [ROADMAP.md](../../ROADMAP.md)。

## 当前 v1.2.19 基线

当前基线使用 `Xenova/bge-small-zh-v1.5`（512 维）。`sqlite-vec` v3 是 macOS、Windows 和 Linux 桌面构建默认启用的有界 KNN 后端；扩展不可用时检索会明确报告状态并保留 FTS，不会回退为 Rust 全表 cosine 扫描。AI retrieval broker 还融合 FTS、链接、锚点和法规候选。

评测从 `hybrid_retrieve → Rank → scope → ContextPacket` 全链路执行。发布资产只面向 macOS + Windows；Ubuntu/Linux 只作为 CI 与编排 runner，继续执行 sqlite-vec 质量验证，但绝不构建、上传或发布 Linux 包。

## Fixture 与标签

`fixtures/rag-v2-vault/` 将包含 48 篇合成 Markdown 笔记，覆盖相近主题、长文、精确法规、tags/aliases、链接、多文档任务和干扰项。标签集共 60 条：

- 20 条语义硬负例；
- 10 条关键词/精确命中；
- 10 条 tags/aliases；
- 10 条链接/多文档；
- 10 条无答案。

fixture 只用于测试，不包含真实用户笔记或秘密。当前 RAG v2 数据集是**冻结于 v1.2.6 的历史 fixture**；其 labels hash 固定在 `fixtures/rag-v2-vault/fixture-metadata.json`，v1.2.19 仅以它评估当前 broker，不声称数据集已更新为 v1.2.19。旧 `semantic-vault` 是 v1.2.5 历史基线，待 v2 fixture 落地后整体替换，不再扩充。

## 指标与发布门槛

每次评测保存机器、commit、模型、索引状态、查询标签和结果 JSON。固定基线为 `docs/eval/results/v1.2.5-hybrid.json`。

| 指标                           | v1.2.19 门槛                               |
| ------------------------------ | ------------------------------------------ |
| semantic-only Recall@5/30      | ≥ 0.80 / ≥ 0.95                            |
| vector-only Recall@5/30        | ≥ 0.80 / ≥ 0.95（已供给 BGE + sqlite-vec） |
| hybrid any-source Recall@5/30  | ≥ 0.95 / ≥ 0.98（已供给 BGE + sqlite-vec） |
| all-required Recall@5/30       | ≥ 0.90 / ≥ 0.95（已供给 BGE + sqlite-vec） |
| scope 泄漏                     | 0                                          |
| ContextPacket span/hash 有效性 | 100%                                       |
| 无答案 false-positive rate     | ≤ 0.10                                     |
| nDCG@10                        | ≥ 0.85                                     |
| 50k warm KNN p95               | ≤ 750ms（声明的参考机）                    |
| 端到端 retrieval p95           | ≤ 1s（声明的参考机）                       |

默认 FTS gate 与已供给模型的 sqlite-vec gate 独立执行：后者由 macOS/Windows
打包流水线显式调用，缺少已验证模型或扩展即失败，不能以 FTS 结果替代。已供给 gate 对每条
fixture 查询再执行 vector-only broker 调用，要求 `vector_chunks=ok`、向量结果命中并通过
向量包 span/hash 100% 校验。50k scale ladder 只在 Ubuntu CI 的临时目录写入确定性生成
manifest，并在内存 SQLite 数据库物化合成记录；输出 revision、模型指纹、平台、每个规模的
fixture 生成哈希、参考机与原始样本；发布 p95
仅使用独立 50k 样本，绝不混入 1k/10k/25k。tag 的 `release-quality` 还会在同一 commit
同步执行该 50k gate，Windows/macOS 打包必须等待其通过。它不构建、上传或发布任何 Linux 桌面包。评测失败不得以“模型下载、
sqlite-vec 未启用或候选不足”跳过并宣称通过；必须明确记录降级状态和失败原因。
