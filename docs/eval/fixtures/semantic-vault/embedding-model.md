---
title: 嵌入模型
tags: [搜索, AI]
---

# 向量嵌入模型

v0.1 使用 fastembed 的 all-MiniLM-L6-v2 生成 384 维向量，写入 chunk_embeddings 表。查询时同样 embed 后做余弦排序。
