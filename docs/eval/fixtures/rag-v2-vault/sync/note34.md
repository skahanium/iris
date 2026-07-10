---
title: "操作转换算法与协同编辑"
aliases: ["OT", "操作转换", "operational-transformation", "协同编辑算法"]
tags: ["area-sync", "fixture", "分布式同步", "OT", "协同编辑"]
---

# 操作转换算法与协同编辑

操作转换（Operational Transformation, OT）是实时协同编辑系统的经典算法基础。与 CRDT 的无协调收敛不同，OT 通过中心化服务器来线性化所有操作序列，当并发操作发生冲突时，服务器负责将接收到的操作相对于已执行操作进行"变换"（Transform），然后将变换后的操作广播给所有客户端。

OT 的核心是包含函数（Inclusion）和排除函数（Exclusion）。当服务器接收到操作 Op1 和 Op2 且两者在时间上存在重叠时，需要计算 `transform(Op1, Op2)` 的结果，即"相对于 Op2 已在文档中执行后的 Op1"。其直观含义是：将 Op1 的操作位置按照 Op2 的效果进行平移调整。例如，如果 Op2 在 Op1 之前的位置插入了 3 个字符，那么 Op1 的字符位置应向后偏移 3。

证据令牌: evaltok34

经典 OT 算法包括 dOPT、GOT（通用 OT）、Jupiter（Google Docs 的前身 Google Wave 使用）和 COT（上下文 OT）。Google Docs 当前的协同架构实际上采用了 OT 与 CRDT 混合的策略：核心文本操作使用 OT 保证顺序语义，部分结构化数据使用 CRDT 简化去中心化处理。这种混合方案结合了 OT 的有序性优势和 CRDT 的无协调优势。

OT 的主要局限在于对中心化服务器的依赖和算法正确性证明的难度。大多数 OT 实现假设服务器为单一信源（Single Source of Truth），当服务器不可用时客户端无法继续编辑。此外，证明一个 OT 变换函数在所有操作组合下都满足收敛性（Convergence Property）和因果保持性（Causality Preservation）是一个非平凡的数学问题，历史上多个 OT 实现都曾被发现有收敛性缺陷。
