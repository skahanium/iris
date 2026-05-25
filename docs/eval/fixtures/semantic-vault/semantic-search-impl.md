---
title: 语义搜索实现
tags: [搜索]
---

# 语义搜索实现说明

IPC `search_semantic` 对全部 chunk 计算相似度，返回 Top-K 路径与片段。AiPanel 提问前可注入关联笔记 Top-5。
