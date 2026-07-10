---
title: "CRDT无冲突复制数据类型原理"
aliases: ["CRDT", "无冲突数据类型", "conflict-free-replicated-data-types"]
tags: ["area-sync", "fixture", "分布式同步", "CRDT", "数据一致性"]
---

# CRDT无冲突复制数据类型原理

CRDT（Conflict-free Replicated Data Types）是一类专为分布式系统中无需协调即可达到最终一致性的数据结构。它们最核心的设计原则是：任何两个副本独立接收的并发更新操作，在通过网络同步交换后总是能自动合并为一致的状态，无需人工解决冲突也不需要中心化协调节点。

CRDT 分为两大类：基于状态（State-based，简称 CvRDT）和基于操作（Operation-based，简称 CmRDT）。CvRDT 通过传播副本的完整状态并通过一个满足结合律、交换律和幂等律的合并函数（Merge）来收敛。CmRDT 则传播增量操作本身，这些操作被设计为可交换的，因此无论操作以何种顺序到达远程副本，最终状态总是一致的。

证据令牌: evaltok33

计数器是 CRDT 最直观的示例。传统的整数计数器不支持并发增减的合并。G-Counter（增长计数器）通过为每个副本分配独立的计数值槽位，合并时取各槽位的最大值再求和来实现单调递增。PN-Counter（正负计数器）在 G-Counter 基础上为增量和减量分别维护独立的 G-Counter，通过增量总和减去减量总和来支持增减操作。

CRDT 的实际应用已深入到日常工具中。Yjs 和 Automerge 是基于 CRDT 的协同编辑库，支持多个用户实时编辑同一文档。Apple Notes 使用 CRDT 实现多设备间的笔记同步。与 [[note34]] 中讨论的 OT 方法相比，CRDT 的去中心化特性更适合点对点（P2P）网络和间歇性连接的移动场景。参见 [[note35]] 关于最终一致性模型的理论基础。
