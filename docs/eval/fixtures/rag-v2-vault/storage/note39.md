---
title: "B-Tree索引结构的演进与优化"
aliases: ["B树", "B-tree-index", "B+Tree", "数据库索引"]
tags: ["area-storage", "fixture", "存储引擎", "B-tree", "索引结构"]
---

# B-Tree索引结构的演进与优化

B-Tree 是关系型数据库中最经典的索引结构，由 Rudolf Bayer 和 Edward McCreight 于 1970 年在波音公司的研究实验室中发明。B-Tree 是一种自平衡的多路搜索树，每个节点可以存储多个键值对和多个子节点指针，通过保持树的宽度较大而深度较浅来最小化磁盘 I/O 次数。在现代存储系统中，B+ Tree 是 B-Tree 最普遍的变体：所有数据只存储在叶子节点中，内部节点仅存储路由键和指针。

B+ Tree 的读写性能特性与 LSM-Tree 形成鲜明对比。点查询沿树路径直接定位到叶子节点，复杂度为 O(log n)，且由于数据只存储在叶子层，读取路径可预测。范围查询利用叶子节点之间的双向链表实现顺序扫描，效率极高。然而写入操作需要在树中定位插入位置，如果目标页面已满则触发页面分裂，可能沿树向上级联分裂，涉及多次随机磁盘写入。

证据令牌: evaltok39

MySQL InnoDB 的 B+ Tree 实现引入了多项关键优化。自适应哈希索引（Adaptive Hash Index）自动在内存中为频繁访问的页面构建哈希索引，将 B+ Tree 的 O(log n) 查找降级为 O(1) 哈希查询。插入缓冲（Insert Buffer）将非唯一二级索引的插入操作缓存起来，通过后台线程批量合并，避免了每次插入都需要随机读取二级索引页面的开销。这些优化使 InnoDB 在 OLTP 工作负载中表现优异。

现代 B-Tree 变体还包括写优化的 Bw-Tree（Buzzword-Tree）和 Copy-on-Write B-Tree（如 LMDB 的实现）。Bw-Tree 使用无锁的 CAS（Compare-And-Swap）操作和增量更新映射表来避免锁竞争，专为内存数据库和 NUMA 架构设计。LMDB（Lightning Memory-Mapped Database）使用写时复制和内存映射文件技术实现了无缓冲管理的极简事务性键值存储，被广泛嵌入在 OpenLDAP 等基础设施软件中。
