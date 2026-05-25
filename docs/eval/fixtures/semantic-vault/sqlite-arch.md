---
title: SQLite 架构选型
tags: [存储, 架构]
---

# SQLite 架构选型

Iris v0.1 使用 rusqlite 存储文件元数据、FTS5 全文索引与 chunk_embeddings BLOB。向量检索在 Rust 侧做余弦相似度，而非独立向量数据库进程。
