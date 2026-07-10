---
title: "LSM-Tree存储引擎的架构设计"
aliases: ["LSM树", "log-structured-merge-tree", "LSM架构"]
tags: ["area-storage", "fixture", "存储引擎", "LSM-tree", "数据库内核"]
---

# LSM-Tree存储引擎的架构设计

LSM-Tree（Log-Structured Merge-Tree）是一种专为高写入吞吐量场景设计的存储引擎数据结构，由 Patrick O'Neil 等人于 1996 年提出。LSM-Tree 的核心思想是将随机写操作转化为顺序写操作：所有写入首先追加到内存中的 MemTable，当 MemTable 达到容量阈值后将其整体刷写（Flush）至磁盘形成不可变的 SSTable（Sorted String Table）文件。

LSM-Tree 的写入路径极短且高效。写入操作仅需先写入 WAL（预写日志）以保证持久性，再将键值对插入内存中的 MemTable 即可返回客户端，无任何磁盘寻道操作。MemTable 通常实现为跳表（Skip List）或红黑树以保证有序插入和对数复杂度的查询。这种写入优化使得 LSM-Tree 特别适合日志数据、时序数据、物联网传感器数据等写入密集型工作负载。

证据令牌: evaltok37

LSM-Tree 的读取路径相对复杂，因为数据可能分散在多个层级中：当前活跃的 MemTable、等待刷写的 Immutable MemTable 以及磁盘上多个层的 SSTable 文件。读取操作需要按从新到旧的顺序依次查询，为加速查找，通常在每个 SSTable 中维护布隆过滤器（Bloom Filter）和稀疏索引。布隆过滤器能以少量内存代价快速排除不包含目标键的 SSTable 文件，显著减少无效的磁盘 I/O。

LSM-Tree 在 LevelDB、RocksDB、Apache Cassandra 和 Apache HBase 等存储系统中得到了广泛应用。RocksDB 在 Facebook 的持续优化下增加了通用压缩（Universal Compaction）、直接 I/O 支持、列族和生存时间（TTL）等企业级特性，成为现代数据库存储引擎的标杆实现。与 [[note39]] 中讨论的 B-tree 相比，LSM-Tree 在写入性能上有数量级优势，但在点查询和范围查询延迟方面通常不及 B-tree。
